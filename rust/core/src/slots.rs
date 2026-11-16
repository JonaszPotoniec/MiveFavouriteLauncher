use crate::layout::SLOTS;

/// The twelve grid slots, each an activity as "package/class" or empty.
/// Stored as a text file with one line per slot, in keypad order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Slots(pub [Option<String>; SLOTS]);

impl Slots {
    pub fn parse(text: &str) -> Slots {
        let mut slots = Slots::default();
        for (slot, line) in slots.0.iter_mut().zip(text.lines()) {
            let line = line.trim();
            if is_component(line) {
                *slot = Some(line.to_string());
            }
        }
        slots
    }

    pub fn serialize(&self) -> String {
        let mut out = String::new();
        for slot in &self.0 {
            out.push_str(slot.as_deref().unwrap_or(""));
            out.push('\n');
        }
        out
    }

    pub fn get(&self, i: usize) -> Option<&str> {
        self.0.get(i)?.as_deref()
    }

    pub fn set(&mut self, i: usize, component: Option<String>) {
        if let Some(slot) = self.0.get_mut(i) {
            *slot = component.filter(|c| is_component(c));
        }
    }
}

/// Splits "package/class" into its two halves.
pub fn split_component(c: &str) -> Option<(&str, &str)> {
    c.split_once('/').filter(|(p, n)| !p.is_empty() && !n.is_empty())
}

fn is_component(s: &str) -> bool {
    split_component(s).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_ignores_garbage() {
        let mut s = Slots::default();
        s.set(0, Some("org.telegram.messenger.web/org.telegram.messenger.DefaultIcon".into()));
        s.set(11, Some("a/b".into()));
        s.set(5, Some("no-slash".into()));
        let text = s.serialize();
        assert_eq!(text.lines().count(), SLOTS);
        assert_eq!(Slots::parse(&text), s);
        assert_eq!(s.get(5), None);
        assert_eq!(Slots::parse("x/y\n\n/bad\n").get(0), Some("x/y"));
        assert_eq!(Slots::parse("x/y\n\n/bad\n").get(2), None);
    }
}
