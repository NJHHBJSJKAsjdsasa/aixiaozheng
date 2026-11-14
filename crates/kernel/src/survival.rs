//! 生存自动机：饿、渴、累这三件有唯一正确答案的事，由内核直接安排，不问模型。
//!
//! 只在「醒着、空闲、没有别的安排、身体还能动」时接手；正在做事、正在交谈、
//! 被玩家控制的村民一律不打扰。产出的是普通动作组，与模型动作走同一条
//! `World::run_actions`（校验、记账、录像都是同一条路，没有旁路）。
//!
//! 这一轮的想法取需求自己的分级文字（`NeedState::text`，如「很饿」），接管时记下它，
//! 村民不至于一言不发地走开。
//!
//! 健康不在接管范围：它靠休养、吃喝与时间恢复，没有一个「去做某事就回血」的动作。

use crate::activity::ActKind;
use crate::actor::Actor;
use crate::affordance::Spot;
use crate::intent::{BuiltinVerb, StopCond};
use crate::path::{route, StartPoint};
use crate::props::Need;
use crate::World;
use protocol::{ActionDraft, IntentDraft, NeedBand};

/// 一次接管：为什么、做什么、大概多久。
pub struct Takeover {
    /// 调用方记成本轮想法的自述（「很饿，先吃干粮。」）
    pub thought: String,
    /// 交给 `run_actions` 的动作组：需要走动时一项「前往」加一项动手
    pub drafts: Vec<ActionDraft>,
    /// 路程与动作时间之和，用于日志（接管之后这一趟大概要多久）
    pub minutes: f64,
    /// 已经是失控档（不只是紧急）：身体在掉健康，等不起模型那份回复
    pub pressing: bool,
}

/// 该不该接手、怎么接手。纯计算，不改世界；`None` 表示这一轮交给模型。
///
/// 最紧急的那一项如果没处可解（比如镇上根本没有水），就退到下一项去安排；
/// 全都解不了才交回模型——模型可以去找人、去买、去远处打水。
pub fn takeover(world: &World, actor_id: &str) -> Option<Takeover> {
    let a = world.actor(actor_id)?;
    if !available(a) {
        return None;
    }
    for (need, band, text) in urges(a) {
        let Some((locale, spot, minutes)) = pick(world, a, need) else {
            continue;
        };
        let mut drafts = Vec::new();
        if a.locus.locale != locale {
            drafts.push(goto(locale));
        }
        drafts.push(work(locale, spot));
        return Some(Takeover {
            thought: thought(band, &text, &spot.verb),
            drafts,
            minutes,
            pressing: band == NeedBand::Collapse,
        });
    }
    None
}

/// 这一轮的想法：用需求自己那句话的头一段（「很渴，需要饮水」→「很渴」），后面接上要去做的事。
/// 失控档那句是一整条解释（「严重饥饿，持续低值会损害健康」），换成短的一句。
fn thought(band: NeedBand, text: &str, verb: &str) -> String {
    let why = if band == NeedBand::Collapse {
        "身体撑不住了"
    } else {
        text.split('，').next().unwrap_or(text)
    };
    format!("{why}，先{verb}。")
}

/// 只接手「站着发呆」的人：有计划、有动作、在交谈、失能、被玩家接管的都不碰。
fn available(a: &Actor) -> bool {
    a.actions.agent_controlled
        && a.present()
        && a.possession.is_none()
        && a.needs.can_move()
        && a.plan.is_empty()
        && a.current.action.is_none()
        && a.current.interaction.is_none()
        && matches!(
            a.current.kind,
            ActKind::Placeholder {
                cond: StopCond::Never
            }
        )
}

/// 紧急及以上的需求，越往后越优先，同一档数值低的排前面。
fn urges(a: &Actor) -> Vec<(Need, NeedBand, String)> {
    let mut list: Vec<_> = a
        .needs
        .states()
        .into_iter()
        .filter(|s| s.band >= NeedBand::Urgent)
        .filter(|s| s.kind != Need::Health)
        .collect();
    list.sort_by(|x, y| y.band.cmp(&x.band).then(x.value.total_cmp(&y.value)));
    list.into_iter().map(|s| (s.kind, s.band, s.text)).collect()
}

/// 最划算的一趟：路程加动作时长最短；走不过去、做不了的都跳过。
///
/// `each_spot` 按处所成组给出，同一处所只算一次路（寻路是这里唯一的重活）。
fn pick<'a>(world: &'a World, a: &Actor, need: Need) -> Option<(&'a str, &'a Spot, f64)> {
    let start = StartPoint::at(&world.topo, &a.locus);
    let mut best: Option<(&'a str, &'a Spot, f64)> = None;
    let mut walked: Option<(&str, f64)> = None;
    for (locale, spot) in world.affordances.each_spot() {
        if !spot.restores(need) || !world.topo.is_dwell(locale) {
            continue;
        }
        let journey = match walked {
            Some((at, minutes)) if at == locale => minutes,
            _ => {
                // 走不到的处所记成无穷远，同一处所后面的道具不再重复寻路
                let minutes = route(&world.topo, &start, locale, None)
                    .map_or(f64::INFINITY, |journey| journey.total_minutes());
                walked = Some((locale, minutes));
                minutes
            }
        };
        if !journey.is_finite() {
            continue;
        }
        let total = journey + spot.minutes;
        if best.is_none_or(|(_, _, shortest)| total < shortest) {
            best = Some((locale, spot, total));
        }
    }
    best
}

/// 「前往」：内建动词，落脚点留空由内核按处所取缺省。
fn goto(locale: &str) -> ActionDraft {
    ActionDraft {
        intent: IntentDraft {
            verb: BuiltinVerb::Goto.label().into(),
            locale: Some(locale.into()),
            target: None,
            duration_minutes: None,
            until: None,
        },
        inventory: None,
        prop_id: None,
        recipient: None,
        writing: None,
    }
}

/// 用某处所的某件道具做某个动作，时长取道具标称值。
fn work(locale: &str, spot: &Spot) -> ActionDraft {
    ActionDraft {
        intent: IntentDraft {
            verb: spot.verb.clone(),
            locale: Some(locale.into()),
            target: None,
            duration_minutes: Some(spot.minutes),
            until: None,
        },
        inventory: None,
        prop_id: Some(spot.prop.clone()),
        recipient: None,
        writing: None,
    }
}
