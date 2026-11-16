//! Every call into the Android framework goes through here. Only framework classes are used,
//! which the system class loader can find from a native thread.

use jni::objects::{JObject, JString};
use jni::{Env, JValue, JavaVM, jni_sig, jni_str};
use mivefav_core::icon::Icon;
use mivefav_core::slots::split_component;
use mivefav_core::ui::PickerItem;

/// Intent flags a launcher uses: a new task, or the app's existing task brought to the front.
const FLAG_NEW_TASK: i32 = 0x1000_0000;
const FLAG_RESET_TASK_IF_NEEDED: i32 = 0x0020_0000;

/// Handle to the VM and the NativeActivity object. `Clone + Send`, so a worker thread can use it.
#[derive(Clone)]
pub struct Jni {
    vm: JavaVM,
    /// A global reference owned by the NativeActivity glue, valid until the activity is destroyed.
    activity: usize,
}

impl Jni {
    pub fn new(app: &android_activity::AndroidApp) -> Jni {
        // SAFETY: android-activity gives a valid JavaVM pointer for the life of the process.
        let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) };
        Jni { vm, activity: app.activity_as_ptr() as usize }
    }

    /// Runs `f` with an attached thread, a fresh local-reference frame and the activity.
    /// A Java exception is logged and cleared, and turns into `None`.
    fn call<T>(&self, what: &str, f: impl FnOnce(&mut Env, &JObject) -> jni::errors::Result<T>) -> Option<T> {
        let r = self.vm.attach_current_thread(|env| {
            env.with_local_frame(32, |env| {
                // SAFETY: `activity` is a live global reference (see the field comment).
                let activity = unsafe { JObject::from_raw(env, self.activity as jni::sys::jobject) };
                let r = f(env, &activity);
                if r.is_err() && env.exception_check() {
                    env.exception_describe();
                    env.exception_clear();
                }
                r
            })
        });
        r.map_err(|e: jni::errors::Error| log::warn!("{what}: {e}")).ok()
    }

    /// Starts `component` the way a launcher does. False when it can't be started, e.g. the
    /// app was uninstalled.
    pub fn launch(&self, component: &str) -> bool {
        let Some((pkg, cls)) = split_component(component) else { return false };
        self.call("launch", |env, activity| {
            let intent = new_intent(env, "android.intent.action.MAIN")?;
            let cat = env.new_string("android.intent.category.LAUNCHER")?;
            env.call_method(&intent, jni_str!("addCategory"), jni_sig!("(Ljava/lang/String;)Landroid/content/Intent;"), &[JValue::Object(&cat)])?;
            let (p, c) = (env.new_string(pkg)?, env.new_string(cls)?);
            env.call_method(
                &intent,
                jni_str!("setClassName"),
                jni_sig!("(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;"),
                &[JValue::Object(&p), JValue::Object(&c)],
            )?;
            env.call_method(&intent, jni_str!("addFlags"), jni_sig!("(I)Landroid/content/Intent;"), &[JValue::Int(FLAG_NEW_TASK | FLAG_RESET_TASK_IF_NEEDED)])?;
            env.call_method(activity, jni_str!("startActivity"), jni_sig!("(Landroid/content/Intent;)V"), &[JValue::Object(&intent)])?;
            Ok(())
        })
        .is_some()
    }

    /// Sends our task to the back without finishing, so the process stays warm.
    pub fn move_task_to_back(&self) {
        self.call("moveTaskToBack", |env, activity| {
            env.call_method(activity, jni_str!("moveTaskToBack"), jni_sig!("(Z)Z"), &[JValue::Bool(true)])?;
            Ok(())
        });
    }

    /// The activity's label, as the launcher shows it.
    pub fn label(&self, component: &str) -> Option<String> {
        let (pkg, cls) = split_component(component)?;
        self.call("label", |env, activity| {
            let pm = package_manager(env, activity)?;
            let cn = component_name(env, pkg, cls)?;
            let info = env
                .call_method(&pm, jni_str!("getActivityInfo"), jni_sig!("(Landroid/content/ComponentName;I)Landroid/content/pm/ActivityInfo;"), &[JValue::Object(&cn), JValue::Int(0)])?
                .l()?;
            load_label(env, &info, &pm)
        })
    }

    /// The activity's icon rendered at `size`×`size` pixels.
    pub fn icon(&self, component: &str, size: i32) -> Option<Icon> {
        let (pkg, cls) = split_component(component)?;
        self.call("icon", |env, activity| {
            let pm = package_manager(env, activity)?;
            let cn = component_name(env, pkg, cls)?;
            let drawable = env
                .call_method(&pm, jni_str!("getActivityIcon"), jni_sig!("(Landroid/content/ComponentName;)Landroid/graphics/drawable/Drawable;"), &[JValue::Object(&cn)])?
                .l()?;
            render_drawable(env, &drawable, size)
        })
        .flatten()
    }

    /// When the package was last installed or updated, in ms since the epoch.
    pub fn last_update(&self, component: &str) -> Option<i64> {
        let (pkg, _) = split_component(component)?;
        self.call("lastUpdateTime", |env, activity| {
            let pm = package_manager(env, activity)?;
            let p = env.new_string(pkg)?;
            let info = env
                .call_method(&pm, jni_str!("getPackageInfo"), jni_sig!("(Ljava/lang/String;I)Landroid/content/pm/PackageInfo;"), &[JValue::Object(&p), JValue::Int(0)])?
                .l()?;
            env.get_field(&info, jni_str!("lastUpdateTime"), jni_sig!("J"))?.j()
        })
    }

    /// Every launcher activity except our own, sorted by label.
    pub fn launcher_apps(&self, own_package: &str) -> Vec<PickerItem> {
        let mut apps = self
            .call("queryIntentActivities", |env, activity| {
                let pm = package_manager(env, activity)?;
                let intent = new_intent(env, "android.intent.action.MAIN")?;
                let cat = env.new_string("android.intent.category.LAUNCHER")?;
                env.call_method(&intent, jni_str!("addCategory"), jni_sig!("(Ljava/lang/String;)Landroid/content/Intent;"), &[JValue::Object(&cat)])?;
                let list = env
                    .call_method(&pm, jni_str!("queryIntentActivities"), jni_sig!("(Landroid/content/Intent;I)Ljava/util/List;"), &[JValue::Object(&intent), JValue::Int(0)])?
                    .l()?;
                let n = env.call_method(&list, jni_str!("size"), jni_sig!("()I"), &[])?.i()?;
                let mut out = Vec::with_capacity(n.max(0) as usize);
                for i in 0..n {
                    let item = env.with_local_frame(16, |env| -> jni::errors::Result<_> {
                        let ri = env.call_method(&list, jni_str!("get"), jni_sig!("(I)Ljava/lang/Object;"), &[JValue::Int(i)])?.l()?;
                        let ai = env.get_field(&ri, jni_str!("activityInfo"), jni_sig!("Landroid/content/pm/ActivityInfo;"))?.l()?;
                        let pkg = string_field(env, &ai, jni_str!("packageName"))?;
                        let cls = string_field(env, &ai, jni_str!("name"))?;
                        let label = load_label(env, &ri, &pm)?;
                        Ok(PickerItem { component: Some(format!("{pkg}/{cls}")), label })
                    })?;
                    out.push(item);
                }
                Ok(out)
            })
            .unwrap_or_default();
        apps.retain(|a| a.component.as_deref().and_then(split_component).is_some_and(|(p, _)| p != own_package));
        apps.sort_by_cached_key(|a| a.label.to_lowercase());
        apps
    }
}

fn new_intent<'l>(env: &mut Env<'l>, action: &str) -> jni::errors::Result<JObject<'l>> {
    let a = env.new_string(action)?;
    env.new_object(jni_str!("android/content/Intent"), jni_sig!("(Ljava/lang/String;)V"), &[JValue::Object(&a)])
}

fn package_manager<'l>(env: &mut Env<'l>, activity: &JObject) -> jni::errors::Result<JObject<'l>> {
    env.call_method(activity, jni_str!("getPackageManager"), jni_sig!("()Landroid/content/pm/PackageManager;"), &[])?.l()
}

fn component_name<'l>(env: &mut Env<'l>, pkg: &str, cls: &str) -> jni::errors::Result<JObject<'l>> {
    let (p, c) = (env.new_string(pkg)?, env.new_string(cls)?);
    env.new_object(
        jni_str!("android/content/ComponentName"),
        jni_sig!("(Ljava/lang/String;Ljava/lang/String;)V"),
        &[JValue::Object(&p), JValue::Object(&c)],
    )
}

/// `PackageItemInfo.loadLabel(pm).toString()` (also works on ResolveInfo, which has the same method).
fn load_label(env: &mut Env, info: &JObject, pm: &JObject) -> jni::errors::Result<String> {
    let cs = env
        .call_method(info, jni_str!("loadLabel"), jni_sig!("(Landroid/content/pm/PackageManager;)Ljava/lang/CharSequence;"), &[JValue::Object(pm)])?
        .l()?;
    let s = env.call_method(&cs, jni_str!("toString"), jni_sig!("()Ljava/lang/String;"), &[])?.l()?;
    to_string(env, s)
}

fn string_field(env: &mut Env, obj: &JObject, name: &jni::strings::JNIStr) -> jni::errors::Result<String> {
    let s = env.get_field(obj, name, jni_sig!("Ljava/lang/String;"))?.l()?;
    to_string(env, s)
}

fn to_string(env: &mut Env, obj: JObject) -> jni::errors::Result<String> {
    let s: JString = env.cast_local::<JString>(obj)?;
    s.try_to_string(env)
}

/// Draws a Drawable into a new ARGB_8888 bitmap and reads the pixels back.
fn render_drawable(env: &mut Env, drawable: &JObject, size: i32) -> jni::errors::Result<Option<Icon>> {
    let config = env
        .get_static_field(jni_str!("android/graphics/Bitmap$Config"), jni_str!("ARGB_8888"), jni_sig!("Landroid/graphics/Bitmap$Config;"))?
        .l()?;
    let bitmap = env
        .call_static_method(
            jni_str!("android/graphics/Bitmap"),
            jni_str!("createBitmap"),
            jni_sig!("(IILandroid/graphics/Bitmap$Config;)Landroid/graphics/Bitmap;"),
            &[JValue::Int(size), JValue::Int(size), JValue::Object(&config)],
        )?
        .l()?;
    let canvas = env.new_object(jni_str!("android/graphics/Canvas"), jni_sig!("(Landroid/graphics/Bitmap;)V"), &[JValue::Object(&bitmap)])?;
    env.call_method(drawable, jni_str!("setBounds"), jni_sig!("(IIII)V"), &[JValue::Int(0), JValue::Int(0), JValue::Int(size), JValue::Int(size)])?;
    env.call_method(drawable, jni_str!("draw"), jni_sig!("(Landroid/graphics/Canvas;)V"), &[JValue::Object(&canvas)])?;
    let n = (size * size) as usize;
    let array = env.new_int_array(n)?;
    env.call_method(
        &bitmap,
        jni_str!("getPixels"),
        jni_sig!("([IIIIIII)V"),
        &[JValue::Object(&array), JValue::Int(0), JValue::Int(size), JValue::Int(0), JValue::Int(0), JValue::Int(size), JValue::Int(size)],
    )?;
    let mut px = vec![0i32; n];
    array.get_region(env, 0, &mut px)?;
    env.call_method(&bitmap, jni_str!("recycle"), jni_sig!("()V"), &[])?;
    Ok(Icon::new(size as usize, px.into_iter().map(|p| p as u32).collect()))
}
