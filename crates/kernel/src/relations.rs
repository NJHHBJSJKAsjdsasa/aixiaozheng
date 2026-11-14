//! 人与人之间的往来账：说过话、给过东西、托过事，办成没办成。全从世界事件里累加，
//! 不问模型——模型只要在观察里看到一句话，就知道该找谁、欠谁。
//!
//! 账记在本人身上（`Actor::relations`，键是对方的编号），随 world.json 存。每人的账
//! 只留最近来往的 [`KEEP`] 份，免得越攒越多；越久没来往的越先被挤掉。
use crate::actor::Actor;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 每人最多留几份账。
const KEEP: usize = 8;
/// 观察里最多列几份（再多就是给人看的热闹，不是给本人做判断的依据）。
pub const SHOWN: usize = 3;

/// 一个人对另一个人的账。
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Relation {
    /// 聊过几次
    #[serde(default)]
    pub talks: u32,
    /// 给出去的东西件数（负数表示收到）
    #[serde(default)]
    pub gifts: i32,
    /// 他托我、我办成的
    #[serde(default)]
    pub done_for: u32,
    /// 他托我、我没办成的
    #[serde(default)]
    pub missed_for: u32,
    /// 我托他、他办成的
    #[serde(default)]
    pub done_by: u32,
    /// 我托他、他没办成的
    #[serde(default)]
    pub missed_by: u32,
    /// 他求我、我推了的
    #[serde(default)]
    pub refused_for: u32,
    /// 我求他、他推了的
    #[serde(default)]
    pub refused_by: u32,
    /// 最近一次打交道（模拟分钟）
    #[serde(default)]
    pub last: f64,
}

impl Relation {
    /// 有没有值得说一句的事。没什么往来的不算。
    pub fn is_empty(&self) -> bool {
        self.talks == 0
            && self.gifts == 0
            && self.done_for == 0
            && self.missed_for == 0
            && self.done_by == 0
            && self.missed_by == 0
            && self.refused_for == 0
            && self.refused_by == 0
    }

    /// 一句话：最要紧的那条，写给人看。先是欠着的，再是办成的，最后才是聊过几次。
    pub fn note(&self) -> Option<String> {
        let mut parts = Vec::new();
        if self.missed_for > 0 {
            parts.push(format!(
                "他托过你 {} 件事，有 {} 件没办成",
                self.done_for + self.missed_for,
                self.missed_for
            ));
        } else if self.done_for > 0 {
            parts.push(format!("他托过你 {} 件事，都办成了", self.done_for));
        }
        if self.missed_by > 0 {
            parts.push(format!("你托过他 {} 件事，他没办成", self.missed_by));
        } else if self.done_by > 0 {
            parts.push(format!("你托过他 {} 件事，都办成了", self.done_by));
        }
        if self.refused_for > 0 {
            parts.push(format!("他求过你 {} 件事，你没答应", self.refused_for));
        }
        if self.refused_by > 0 {
            parts.push(format!("你求过他 {} 件事，被推了", self.refused_by));
        }
        if parts.is_empty() {
            if self.gifts > 0 {
                parts.push(format!("你给过他 {} 件东西", self.gifts));
            } else if self.gifts < 0 {
                parts.push(format!("他给过你 {} 件东西", -self.gifts));
            }
        }
        if parts.is_empty() && self.talks > 0 {
            parts.push(format!("聊过 {} 次", self.talks));
        }
        (!parts.is_empty()).then(|| parts.join("；"))
    }

    /// 值得说一句的分量：托付过的事最重，其次是东西，最后才是闲聊。
    fn weight(&self) -> u32 {
        (self.done_for
            + self.missed_for
            + self.done_by
            + self.missed_by
            + self.refused_for
            + self.refused_by)
            * 10
            + self.gifts.unsigned_abs()
            + self.talks
    }
}

/// 一个人的关系账。只经下面这些方法改。
pub type Relations = BTreeMap<String, Relation>;

impl Actor {
    /// 记一笔往来：`change` 改账，`now` 顺手记下时间。空账不留下。
    fn relate(&mut self, other: &str, now: f64, change: impl FnOnce(&mut Relation)) {
        if other == self.id || other.is_empty() {
            return;
        }
        let entry = self.relations.entry(other.to_string()).or_default();
        change(entry);
        entry.last = now;
        if entry.is_empty() {
            self.relations.remove(other);
            return;
        }
        if self.relations.len() > KEEP {
            // 挤掉最久没来往的那份
            if let Some(oldest) = self
                .relations
                .iter()
                .filter(|(id, _)| id.as_str() != other)
                .min_by(|a, b| a.1.last.total_cmp(&b.1.last))
                .map(|(id, _)| id.clone())
            {
                self.relations.remove(&oldest);
            }
        }
    }

    pub(crate) fn note_talk(&mut self, other: &str, now: f64) {
        self.relate(other, now, |r| r.talks = r.talks.saturating_add(1));
    }

    /// 给出去（`out`）还是收到（`!out`）东西，记几件。
    pub(crate) fn note_gift(&mut self, other: &str, now: f64, count: i32, out: bool) {
        let delta = if out { count } else { -count };
        self.relate(other, now, |r| {
            r.gifts = r.gifts.saturating_add(delta);
        });
    }

    /// 他托我：办成（`done`）还是没办成。
    pub(crate) fn note_asked(&mut self, other: &str, now: f64, done: bool) {
        self.relate(other, now, |r| {
            if done {
                r.done_for = r.done_for.saturating_add(1);
            } else {
                r.missed_for = r.missed_for.saturating_add(1);
            }
        });
    }

    /// 他求我，我推了。
    pub(crate) fn note_refused(&mut self, other: &str, now: f64) {
        self.relate(other, now, |r| {
            r.refused_for = r.refused_for.saturating_add(1)
        });
    }

    /// 我求他，他推了。
    pub(crate) fn note_refused_by(&mut self, other: &str, now: f64) {
        self.relate(other, now, |r| {
            r.refused_by = r.refused_by.saturating_add(1)
        });
    }

    /// 我托他：办成（`done`）还是没办成。
    pub(crate) fn note_asked_of(&mut self, other: &str, now: f64, done: bool) {
        self.relate(other, now, |r| {
            if done {
                r.done_by = r.done_by.saturating_add(1);
            } else {
                r.missed_by = r.missed_by.saturating_add(1);
            }
        });
    }

    /// 观察里那几句：分量最重的几份，各带一句最要紧的话。
    pub fn relation_notes(&self, name: &dyn Fn(&str) -> String) -> Vec<(String, String)> {
        let mut entries: Vec<_> = self
            .relations
            .iter()
            .filter(|(_, r)| !r.is_empty())
            .collect();
        entries.sort_by(|a, b| {
            b.1.weight()
                .cmp(&a.1.weight())
                .then(b.1.last.total_cmp(&a.1.last))
        });
        entries
            .into_iter()
            .take(SHOWN)
            .filter_map(|(id, r)| r.note().map(|note| (name(id), note)))
            .collect()
    }
}
