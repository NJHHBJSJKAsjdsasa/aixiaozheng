//! 日程：模型一次排好几件带时刻的事，到点由内核直接执行，中间不必每隔一阵再问一次。
//!
//! 和生存自动机（`survival.rs`）一样，产出的是普通动作组，走同一条 `World::run_actions`：
//! 校验、记账、录像都是同一条路。差别只在「谁来定」——生存是按身体状态定，日程是模型
//! 自己排的，内核只负责守时。
//!
//! 日程是当前状态，不是历史：最多 [`AGENDA_STEPS_MAX`] 件，随 world.json 存。到点之后
//! 这人正忙着（在做事、在交谈、被玩家接管、身体不能动）就先留着，空下来再做；但超过
//! [`AGENDA_LATE_MINUTES`] 还没做上就作废——睡了一夜再去做下午的事没有意义。
//!
//! 被打断就整份作废（`World::interrupt_actions`）：有人叫住你、被推了一把，计划就变了，
//! 剩下的交给模型重排。

use crate::action_check::normalize_drafts;
use crate::action_state::GROUP_LIMIT;
use crate::error::{KernelError, Result};
use crate::World;
use protocol::ActionDraft;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// 一份日程最多排几件。
pub const AGENDA_STEPS_MAX: usize = 4;
/// 到点之后最多等多久；再晚就作废。
pub const AGENDA_LATE_MINUTES: f64 = 90.0;

/// 一件事：到这一刻开始做这一组动作。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    /// 到这一刻（模拟分钟）开始做
    pub at: f64,
    pub drafts: Vec<ActionDraft>,
}

/// 一个人的日程，按时刻从早到晚。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Agenda {
    #[serde(default, skip_serializing_if = "VecDeque::is_empty")]
    steps: VecDeque<Step>,
}

impl Agenda {
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// 下一件的时刻与动作；没有日程时为 None。
    pub fn next(&self) -> Option<&Step> {
        self.steps.front()
    }
}

impl World {
    /// 换一份日程。时刻由调用方算好（模拟分钟），须严格递增且都在此刻之后。
    /// 返回排好的件数。
    pub fn set_agenda(
        &mut self,
        actor: &str,
        steps: Vec<(f64, Vec<ActionDraft>)>,
    ) -> Result<usize> {
        ensure!(
            self.actors.contains_key(actor),
            KernelError::NoSuchActor(actor.into())
        );
        ensure!(
            !steps.is_empty() && steps.len() <= AGENDA_STEPS_MAX,
            KernelError::action(format!("日程须含 1–{AGENDA_STEPS_MAX} 件事"))
        );
        let now = self.minutes;
        let mut agenda = Agenda::default();
        for (at, drafts) in steps {
            ensure!(
                at.is_finite() && at > now,
                KernelError::action("日程的时刻要在此刻之后")
            );
            ensure!(
                !drafts.is_empty() && drafts.len() <= GROUP_LIMIT,
                KernelError::action(format!("每件事须含 1–{GROUP_LIMIT} 项动作"))
            );
            ensure!(
                agenda.steps.back().is_none_or(|last| at > last.at),
                KernelError::action("日程按时刻从早到晚排，不能重复或倒着来")
            );
            agenda.steps.push_back(Step {
                at,
                drafts: normalize_drafts(&self.topo, &drafts),
            });
        }
        if let Some(a) = self.actors.get_mut(actor) {
            a.agenda = agenda;
        }
        Ok(self.actors[actor].agenda.len())
    }

    pub fn agenda(&self, actor: &str) -> Option<&Agenda> {
        self.actors.get(actor).map(|a| &a.agenda)
    }

    /// 下一件还没到点的日程时刻。给「有安排就别再问」和时长的判断用。
    pub fn agenda_next(&self, actor: &str) -> Option<f64> {
        self.actors
            .get(actor)
            .and_then(|a| a.agenda.next())
            .map(|step| step.at)
    }

    /// 清掉整份日程（被打断、被玩家接管、自己改了主意）。
    pub fn clear_agenda(&mut self, actor: &str) {
        if let Some(a) = self.actors.get_mut(actor) {
            a.agenda = Agenda::default();
        }
    }

    /// 到点、且这人此刻真的空着，就把这一件取走交给调用方执行（走 `run_actions`）。
    /// 同时清掉过期作废的几件；忙着的时候什么都取不走，到点的这件留着等他有空。
    pub fn take_due_agenda(&mut self, actor: &str, now: f64) -> Option<Vec<ActionDraft>> {
        let a = self.actors.get_mut(actor)?;
        while a
            .agenda
            .steps
            .front()
            .is_some_and(|step| now > step.at + AGENDA_LATE_MINUTES)
        {
            a.agenda.steps.pop_front();
        }
        let due = a.agenda.steps.front().is_some_and(|step| step.at <= now);
        if !due || !idle(a) {
            return None;
        }
        a.agenda.steps.pop_front().map(|step| step.drafts)
    }
}

/// 站在那儿等日程的人：没有别的安排、没有动作、不在交谈、没被玩家接管、身体还能动。
fn idle(a: &crate::actor::Actor) -> bool {
    a.present()
        && a.possession.is_none()
        && a.needs.can_move()
        && a.plan.is_empty()
        && a.current.action.is_none()
        && a.current.interaction.is_none()
        && matches!(
            a.current.kind,
            crate::activity::ActKind::Placeholder {
                cond: crate::intent::StopCond::Never
            }
        )
}
