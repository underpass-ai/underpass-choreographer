use serde::{Deserialize, Serialize};

/// How one remembered thing explains another.
///
/// Named in this engine's terms and not a memory backend's. What a
/// particular kernel calls these, and how it classes them, is a
/// mapping an adapter owns — an engine that spoke one backend's
/// taxonomy would have to change when that backend did.
///
/// Each kind says who is in a position to assert it, because that is
/// the difference between a reason and a guess. The engine asserts
/// only what it can see in its own aggregate; everything else is
/// asserted by whoever did the reasoning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRelationKind {
    /// This is the reply to what that one asked.
    ///
    /// The engine asserts this: a contribution and the agenda item it
    /// was made against are both in the session, and which answers
    /// which is not a judgement.
    Answers,

    /// This was decided because of that.
    ///
    /// Asserted by whoever decided. The engine cannot infer it —
    /// something being written down before a decision is not what made
    /// the decision, and recording it as though it were is how a
    /// coincidence becomes a precedent.
    ChosenBecause,

    /// This was brought about by doing that — **the how**.
    ///
    /// Not a weaker form of cause. A reason says what made something
    /// necessary; this says what was actually done, and it is what
    /// makes a memory repeatable rather than only understandable. A
    /// session that recorded why it resolved and not how leaves a
    /// precedent nobody can turn into a procedure.
    AchievedBy,

    /// This came about because of that.
    ///
    /// Asserted by whoever can vouch for it. The strongest claim
    /// available and the easiest to make carelessly: a session ending
    /// well after an action is not the action having worked.
    FollowsFrom,

    /// This honours a limit that one set.
    SatisfiesConstraint,

    /// This breaks a limit that one set, knowingly.
    ViolatesConstraint,

    /// This replaces that as what is believed now.
    ///
    /// What was superseded is not deleted: a later session asking what
    /// was thought at the time needs the belief that was replaced, and
    /// a session that quietly overwrote it would answer with today's
    /// knowledge about yesterday's decision.
    Supersedes,

    /// These cannot both be true.
    ///
    /// Kept rather than resolved. Two sessions that reached opposite
    /// conclusions are a fact about the problem, and flattening them
    /// into one would lose the only warning a third session gets.
    Contradicts,
}

impl MemoryRelationKind {
    #[must_use]
    pub const fn as_label(self) -> &'static str {
        match self {
            Self::Answers => "answers",
            Self::ChosenBecause => "chosen_because",
            Self::AchievedBy => "achieved_by",
            Self::FollowsFrom => "follows_from",
            Self::SatisfiesConstraint => "satisfies_constraint",
            Self::ViolatesConstraint => "violates_constraint",
            Self::Supersedes => "supersedes",
            Self::Contradicts => "contradicts",
        }
    }

    /// Whether this engine can assert the relation on its own.
    ///
    /// Everything else needs someone who did the reasoning to say so.
    /// Kept as a property rather than as a comment so a caller that
    /// generates relations automatically can be stopped from
    /// generating the ones nobody is entitled to.
    #[must_use]
    pub const fn is_observable_by_the_engine(self) -> bool {
        matches!(self, Self::Answers | Self::Supersedes)
    }

    /// Whether this says **how** something was done rather than why.
    ///
    /// Worth telling apart: a memory kernel measuring how explanatory
    /// a memory is counts causes, motives and evidence, and does not
    /// count method. A session with method and no cause scores zero on
    /// that measure and is not thereby worthless — it is repeatable
    /// and unexplained, which is a different thing from explained and
    /// unrepeatable.
    #[must_use]
    pub const fn is_method(self) -> bool {
        matches!(self, Self::AchievedBy)
    }
}
