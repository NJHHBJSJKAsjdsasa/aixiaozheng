//! 请托：请对方在某个时刻之前做一件事。答应后双方照常活动、不占身体，和约定一样只记着；
//! 做完由世界按动作认下（`settle_asks`），到点还没做成记没办成（`advance_asks`），
//! 双方都收到通知。发起人要认得那个地方，那件事要真做得成——委托不该是许愿。
use super::{Fact, Interactions};
use crate::error::{KernelError, Result};
use crate::World;
use protocol::{FactKind, InteractionKind, InteractionStatus as Status, Task};

/// 请托至少要在这么多分钟以后、最多到一天之内（互动时长不超过一天）。
const MIN_LEAD: f64 = 10.0;
/// 到点前多少分钟提醒一次，好让人安排手上的事。
const REMIND_BEFORE: f64 = 30.0;

/// 请托的内容检查：地点认得、那件事此处做得了、时刻在未来一天内。返回存档里的时长。
pub(crate) fn check_ask(w: &World, actor: &str, task: &Task, at: f64) -> Result<f64> {
    let now = w.minutes;
    ensure!(
        at.is_finite() && at >= now + MIN_LEAD && at <= now + crate::DAY,
        KernelError::interaction(format!(
            "要办的事得排在 {MIN_LEAD:.0} 分钟以后、一天以内，写成 D日数 HH:MM"
        ))
    );
    match task {
        Task::Do { verb, locale } => {
            ensure!(w.remembers(actor, locale), World::unknown_way());
            ensure!(
                w.affordances
                    .options(locale)
                    .iter()
                    .any(|spot| spot.verb == *verb),
                KernelError::interaction(format!(
                    "{}没有「{verb}」这件事可做",
                    w.topo.locale_name(locale)
                ))
            );
        }
        Task::Deliver { prop_id, to } => {
            ensure!(
                w.topo.prop(prop_id).is_some(),
                KernelError::interaction("镇上没有要交的这件东西")
            );
            ensure!(
                to != actor,
                KernelError::interaction("东西不能交给托你的人自己")
            );
            ensure!(
                w.actors.contains_key(to),
                KernelError::interaction("收东西的人不在镇上")
            );
        }
    }
    Ok(at - now)
}

impl World {
    /// 请托的说法：「D2 14:00 前到工坊做「制作器物」」「D2 14:00 前把「包裹」交给小禾」。
    pub fn describe_task(&self, task: &Task, at: f64) -> String {
        let what = match task {
            Task::Do { verb, locale } => format!("去{}做「{verb}」", self.topo.locale_name(locale)),
            Task::Deliver { prop_id, to } => format!(
                "把「{}」交给{}",
                self.props
                    .item(&self.topo, prop_id)
                    .map_or_else(|_| prop_id.clone(), |p| p.name.clone()),
                self.actors.get(to).map_or(to.as_str(), |a| &a.name)
            ),
        };
        format!("{}前{what}", crate::clock_string(at))
    }
}

impl Interactions {
    /// 进行中的请托（已答应、还没做成也没到点）。
    fn asks(&self) -> Vec<(String, Task, f64)> {
        self.items
            .values()
            .filter(|i| i.status == Status::Running)
            .filter_map(|i| match &i.kind {
                InteractionKind::Ask { task, at } => Some((i.id.clone(), task.clone(), *at)),
                _ => None,
            })
            .collect()
    }

    /// 下一次要提醒或判定的时刻还有多久：离 at 还有多久、离提醒还有多久。
    pub(crate) fn ask_boundary(&self, now: f64) -> f64 {
        self.asks()
            .into_iter()
            .flat_map(|(_, _, at)| [at - REMIND_BEFORE, at])
            .filter(|t| *t > now + 1e-9)
            .map(|t| t - now)
            .fold(f64::INFINITY, f64::min)
    }
}

impl World {
    /// 世界从 t0 推进到现在：跨过提醒时刻就说一声；跨过 at 还没做成，记没办成。
    pub(crate) fn advance_asks(&mut self, t0: f64) {
        let now = self.minutes;
        for (id, task, at) in self.interactions.asks() {
            let item = self.interactions[id.as_str()].clone();
            let remind = at - REMIND_BEFORE;
            if t0 < remind && remind <= now + 1e-9 && item.started_at.is_some_and(|s| s < remind) {
                let what = self.describe_task(&task, at);
                for (me, other) in [
                    (&item.initiator, &item.partner),
                    (&item.partner, &item.initiator),
                ] {
                    let name = self.actors[other].name.clone();
                    self.interactions.link_event(
                        &mut self.actors,
                        now,
                        &item.id,
                        me,
                        Fact {
                            kind: FactKind::Notice,
                            text: format!("和{name}说好的事{what}，还有 {REMIND_BEFORE:.0} 分钟"),
                            objects: vec![item.id.clone(), other.clone()],
                        },
                    );
                }
            }
            if now + 1e-9 >= at {
                let reason = format!("到{}还没做成", crate::clock_string(at));
                self.interactions
                    .end(&mut self.actors, now, &id, Status::Expired, Some(reason));
                self.actor_mut(&item.initiator)
                    .note_asked_of(&item.partner, now, false);
                self.actor_mut(&item.partner)
                    .note_asked(&item.initiator, now, false);
            }
        }
    }

    /// 这个人刚把东西交出去：手上有没有一件"把这件交给这个人"的请托。有就认下。
    pub(crate) fn settle_delivery(&mut self, from: &str, to: &str, items: &[String], now: f64) {
        let done: Vec<String> = self
            .interactions
            .asks()
            .into_iter()
            .filter(|(id, task, _)| {
                self.interactions[id.as_str()].partner == from
                    && matches!(task, Task::Deliver { prop_id, to: who }
                        if items.contains(prop_id) && who == to)
            })
            .map(|(id, _, _)| id)
            .collect();
        for id in done {
            let item = self.interactions[id.as_str()].clone();
            let who = self.actors[from].name.clone();
            let taker = self.actors[to].name.clone();
            let by = self.actors[&item.initiator].name.clone();
            self.interactions.fact(
                &mut self.actors,
                now,
                &item,
                &format!("{who}把东西交给{taker}了，{by}托的事办成"),
            );
            self.interactions
                .end(&mut self.actors, now, &id, Status::Succeeded, None);
            self.interactions.link_event(
                &mut self.actors,
                now,
                &id,
                &item.initiator,
                Fact {
                    kind: FactKind::Notice,
                    text: format!("{who}把你托的东西交给{taker}了"),
                    objects: vec![item.id.clone(), from.to_string(), to.to_string()],
                },
            );
            self.actor_mut(&item.initiator)
                .note_asked_of(from, now, true);
            self.actor_mut(from).note_asked(&item.initiator, now, true);
        }
    }

    /// 这个人刚做完一件事：手上有没有一件请托正好是它。有就认下，并告诉托付的人。
    pub(crate) fn settle_asks(&mut self, actor: &str, verb: &str, locale: &str, now: f64) {
        let done: Vec<String> = self
            .interactions
            .asks()
            .into_iter()
            .filter(|(id, task, _)| {
                self.interactions[id.as_str()].partner == actor
                    && match task {
                        Task::Do { verb: v, locale: l } => v == verb && l == locale,
                        Task::Deliver { .. } => false,
                    }
            })
            .map(|(id, _, _)| id)
            .collect();
        for id in done {
            let item = self.interactions[id.as_str()].clone();
            let place = self.topo.locale_name(locale);
            let who = self.actors[actor].name.clone();
            let by = self.actors[&item.initiator].name.clone();
            self.interactions.fact(
                &mut self.actors,
                now,
                &item,
                &format!("{who}在{place}把「{verb}」做完了，{by}托的事办成"),
            );
            self.interactions
                .end(&mut self.actors, now, &id, Status::Succeeded, None);
            // 托付的人可能不在场：给他一条通知，别让他一直等
            self.interactions.link_event(
                &mut self.actors,
                now,
                &id,
                &item.initiator,
                Fact {
                    kind: FactKind::Notice,
                    text: format!("{who}把你托的事办成了：在{place}做完了「{verb}」"),
                    objects: vec![item.id.clone(), actor.to_string()],
                },
            );
            self.actor_mut(&item.initiator)
                .note_asked_of(actor, now, true);
            self.actor_mut(actor).note_asked(&item.initiator, now, true);
        }
    }
}
