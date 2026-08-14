//! The typed change report: every step of a run reports one entry, even when it
//! decides to do nothing.

/// What happens (or would happen) to a file, container, or install target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Created,
    Updated,
    Unchanged,
    Kept,
    Planned,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Created => "created",
            Status::Updated => "updated",
            Status::Unchanged => "unchanged",
            Status::Kept => "kept",
            Status::Planned => "planned",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Change {
    pub subject: String,
    pub status: Status,
    pub note: String,
}

impl Change {
    pub(crate) fn new(subject: impl Into<String>, status: Status, note: impl Into<String>) -> Self {
        Self {
            subject: subject.into(),
            status,
            note: note.into(),
        }
    }
}
