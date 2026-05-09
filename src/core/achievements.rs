// Achievements catalogue + earned-state evaluation.
//
// Same set of seven milestones as the Tauri version. Two categories:
// "lifetime" achievements unlock at a cumulative keystroke count;
// "streak" achievements unlock at a streak length in days. Keeping
// them in a single Vec with a tagged threshold makes the evaluator
// trivial — no per-achievement closure indirection.
//
// IDs are stable strings (not array indices) so adding / removing /
// reordering achievements never invalidates earlier earn records.

#[derive(Debug, Clone, Copy)]
pub enum Threshold {
    Lifetime(i64),
    Streak(i64),
}

#[derive(Debug, Clone)]
pub struct AchievementDef {
    pub id: &'static str,
    /// i18n key for the human title — UI looks this up with `useT()`.
    pub title_key: &'static str,
    /// i18n key for the one-line description.
    pub desc_key: &'static str,
    pub threshold: Threshold,
}

pub fn catalog() -> &'static [AchievementDef] {
    // Order matters only for display (top-to-bottom on the
    // Achievements view). Earned state is keyed by `id`.
    &[
        AchievementDef {
            id: "first_steps",
            title_key: "ach.firstStepsTitle",
            desc_key: "ach.firstStepsDesc",
            threshold: Threshold::Lifetime(100),
        },
        AchievementDef {
            id: "warming_up",
            title_key: "ach.warmingUpTitle",
            desc_key: "ach.warmingUpDesc",
            threshold: Threshold::Lifetime(10_000),
        },
        AchievementDef {
            id: "cruise_control",
            title_key: "ach.cruiseTitle",
            desc_key: "ach.cruiseDesc",
            threshold: Threshold::Lifetime(100_000),
        },
        AchievementDef {
            id: "million_club",
            title_key: "ach.millionTitle",
            desc_key: "ach.millionDesc",
            threshold: Threshold::Lifetime(1_000_000),
        },
        AchievementDef {
            id: "decimillion",
            title_key: "ach.deciTitle",
            desc_key: "ach.deciDesc",
            threshold: Threshold::Lifetime(10_000_000),
        },
        AchievementDef {
            id: "weeklong",
            title_key: "ach.weeklongTitle",
            desc_key: "ach.weeklongDesc",
            threshold: Threshold::Streak(7),
        },
        AchievementDef {
            id: "marathon",
            title_key: "ach.marathonTitle",
            desc_key: "ach.marathonDesc",
            threshold: Threshold::Streak(30),
        },
    ]
}

/// Returns true if the threshold is satisfied by the given lifetime
/// count and current streak length.
pub fn is_satisfied(def: &AchievementDef, lifetime: i64, streak: i64) -> bool {
    match def.threshold {
        Threshold::Lifetime(t) => lifetime >= t,
        Threshold::Streak(t) => streak >= t,
    }
}

/// Numeric progress toward the threshold (current value / target).
/// Returned as `(current, target)` so the UI can render a ratio.
pub fn progress(def: &AchievementDef, lifetime: i64, streak: i64) -> (i64, i64) {
    match def.threshold {
        Threshold::Lifetime(t) => (lifetime.min(t), t),
        Threshold::Streak(t) => (streak.min(t), t),
    }
}
