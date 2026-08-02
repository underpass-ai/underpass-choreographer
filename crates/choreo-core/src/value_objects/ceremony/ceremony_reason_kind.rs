use serde::{Deserialize, Serialize};

/// Who is in a position to assert a reason.
///
/// The difference between a reason and a guess, made checkable. It is
/// not a permission model bolted on — it follows from what each kind
/// of claim is *about*, and so it cuts both ways: the engine may not
/// assert what only a participant can know, and a participant may not
/// assert what only the engine can see.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonAsserter {
    /// The engine, and nobody else.
    ///
    /// These say what the session *is* rather than what anyone
    /// concluded. A participant able to assert them could rewrite the
    /// shape of the session by relabelling it.
    TheEngine,
    /// Whoever produced the record the reason starts from.
    ///
    /// Testimony about one's own reasoning or one's own doing. Nobody
    /// else has access to it, so nobody else may claim it.
    ItsAuthor,
    /// Any seat at this session, saying how sure it is.
    ///
    /// Claims about the world rather than about a mind. Anyone may
    /// make one and everyone may weigh it.
    AnySeat,
}

/// How one thing a session produced explains another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CeremonyReasonKind {
    /// This contribution is the reply to that agenda item.
    Answers,
    /// This was decided because of that.
    ChosenBecause,
    /// That decision is what permitted this action.
    ///
    /// The edge anyone reviewing a session afterwards looks for first:
    /// not what happened, but what made it allowed to happen. A session
    /// that records an action and cannot point at the decision behind
    /// it is a record of events rather than of authority.
    ///
    /// Only whoever made the decision may say what it authorised. That
    /// a third party can see the connection does not make it theirs to
    /// assert: attributing authorising force to somebody else's
    /// decision is the receipt this engine refuses to write.
    Authorizes,
    /// This was brought about by doing that — **the how**.
    ///
    /// The one that turns a resolved session from a precedent into a
    /// procedure. A session that records why it resolved and not how
    /// cannot be turned into anything anyone can repeat.
    AchievedBy,
    /// This came about because of that.
    FollowsFrom,
    /// This honours a limit that one set.
    SatisfiesConstraint,
    /// This breaks a limit that one set, knowingly.
    ViolatesConstraint,
    /// This replaces that as what is believed now.
    Supersedes,
    /// These cannot both be true.
    Contradicts,
}

impl CeremonyReasonKind {
    #[must_use]
    pub const fn as_label(self) -> &'static str {
        match self {
            Self::Answers => "answers",
            Self::Authorizes => "authorizes",
            Self::ChosenBecause => "chosen_because",
            Self::AchievedBy => "achieved_by",
            Self::FollowsFrom => "follows_from",
            Self::SatisfiesConstraint => "satisfies_constraint",
            Self::ViolatesConstraint => "violates_constraint",
            Self::Supersedes => "supersedes",
            Self::Contradicts => "contradicts",
        }
    }

    /// Who may assert a reason of this kind.
    #[must_use]
    pub const fn asserter(self) -> ReasonAsserter {
        match self {
            // Structure. The engine sees which contribution answered
            // which item; it is not judging anything.
            Self::Answers => ReasonAsserter::TheEngine,
            // Testimony. Only whoever decided knows what decided them,
            // and only whoever acted knows how they did it.
            Self::Authorizes | Self::ChosenBecause | Self::AchievedBy => ReasonAsserter::ItsAuthor,
            // Claims about the world, open to anyone and weighable by
            // everyone. The engine is excluded from these on purpose:
            // a session ending well after an action is not the action
            // having worked, and an engine allowed to say otherwise
            // would manufacture precedent out of sequence.
            Self::FollowsFrom
            | Self::SatisfiesConstraint
            | Self::ViolatesConstraint
            | Self::Supersedes
            | Self::Contradicts => ReasonAsserter::AnySeat,
        }
    }

    /// Whether this kind says **how** something was done.
    #[must_use]
    pub const fn is_method(self) -> bool {
        matches!(self, Self::AchievedBy)
    }
}
