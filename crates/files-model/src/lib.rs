#![no_std]

pub const MAX_NAME_BYTES: usize = 32;

pub fn valid_name(name: &[u8]) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME_BYTES
        && name != b"."
        && name != b".."
        && name
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-'))
}

pub fn keep_selected_visible(
    count: usize,
    rows: usize,
    selected: Option<usize>,
    scroll: usize,
) -> (Option<usize>, usize) {
    let rows = rows.max(1);
    let selected = selected.filter(|index| *index < count);
    let maximum = count.saturating_sub(rows);
    let mut scroll = scroll.min(maximum);
    if let Some(selected) = selected {
        if selected < scroll {
            scroll = selected;
        } else if selected >= scroll + rows {
            scroll = selected + 1 - rows;
        }
    }
    (selected, scroll.min(maximum))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClickTracker {
    index: Option<usize>,
    tick: u64,
}

impl ClickTracker {
    pub const fn new() -> Self {
        Self {
            index: None,
            tick: 0,
        }
    }

    pub fn select(&mut self, index: usize, tick: u64, threshold: u64) -> bool {
        let double = self.index == Some(index) && tick.wrapping_sub(self.tick) <= threshold;
        self.index = Some(index);
        self.tick = tick;
        double
    }
}

impl Default for ClickTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeleteDecision {
    Idle,
    ConfirmationRequired(usize),
    Delete(usize),
    Cancelled,
}

pub fn request_delete(selected: Option<usize>) -> DeleteDecision {
    selected.map_or(DeleteDecision::Idle, DeleteDecision::ConfirmationRequired)
}

pub fn confirm_delete(request: DeleteDecision, confirmed: bool) -> DeleteDecision {
    match (request, confirmed) {
        (DeleteDecision::ConfirmationRequired(index), true) => DeleteDecision::Delete(index),
        (DeleteDecision::ConfirmationRequired(_), false) => DeleteDecision::Cancelled,
        (other, _) => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_kernel_vfs_policy() {
        assert!(valid_name(b"notes-2026_08.txt"));
        assert!(!valid_name(b""));
        assert!(!valid_name(b".."));
        assert!(!valid_name(b"nested/file"));
        assert!(!valid_name(&[b'a'; MAX_NAME_BYTES + 1]));
    }

    #[test]
    fn selection_scrolls_into_clipped_view() {
        assert_eq!(keep_selected_visible(17, 5, Some(9), 0), (Some(9), 5));
        assert_eq!(keep_selected_visible(17, 5, Some(2), 8), (Some(2), 2));
        assert_eq!(keep_selected_visible(2, 5, Some(7), 9), (None, 0));
    }

    #[test]
    fn double_click_requires_same_row_within_threshold() {
        let mut clicks = ClickTracker::new();
        assert!(!clicks.select(3, 100, 50));
        assert!(!clicks.select(4, 120, 50));
        assert!(clicks.select(4, 160, 50));
        assert!(!clicks.select(4, 220, 50));
    }

    #[test]
    fn delete_never_occurs_without_explicit_confirmation() {
        let request = request_delete(Some(4));
        assert_eq!(request, DeleteDecision::ConfirmationRequired(4));
        assert_eq!(confirm_delete(request, false), DeleteDecision::Cancelled);
        assert_eq!(confirm_delete(request, true), DeleteDecision::Delete(4));
        assert_eq!(request_delete(None), DeleteDecision::Idle);
    }
}
