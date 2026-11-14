//! 镇上的事：没有具体的人托付，由世界自己说出来的活——井边该清了、工坊缺人做活。
//! 说出来就是全镇的一条通知，谁先做完算谁的，做完再向全镇说一声。
//!
//! 和请托（`interactions/asks.rs`）认的是同一件事、同一套动作证据，区别只在发起人：
//! 请托有具体的人等着回信，镇上的事没有谁在等，只有一个"说完了"的结局。
//! 现在只认「在某处做某件事」；交付型的东西得有人收，算不到镇上。
use crate::World;
use protocol::Task;
use serde::{Deserialize, Serialize};

/// 同时挂着几件。再多就是刷屏，不是给镇子添事。
pub const TOWN_TASKS_MAX: usize = 4;

/// 一件事办成了：谁、什么时候。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Done {
    pub by: String,
    pub at: f64,
}

/// 镇上说的一句话：什么事、什么时候说的、办没办成。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TownTask {
    pub id: String,
    pub task: Task,
    pub posted_at: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done: Option<Done>,
}

impl World {
    /// 把一件事说给全镇听。返回这句话的编号；同类的事已经挂着时不重复说。
    pub fn post_town_task(&mut self, task: Task) -> Option<String> {
        // 同一件事已经挂着、还没做成：不重复
        if self
            .town_tasks
            .iter()
            .any(|t| t.done.is_none() && t.task == task)
        {
            return None;
        }
        let what = self.describe_task_only(&task);
        let id = format!("town-{}", (self.minutes * 10.0) as u64);
        let text = format!("镇上的事：{what}。谁有空谁去，做完说一声。");
        self.announce(&text);
        self.town_tasks.push(TownTask {
            id: id.clone(),
            task,
            posted_at: self.minutes,
            done: None,
        });
        let excess = self.town_tasks.len().saturating_sub(TOWN_TASKS_MAX);
        if excess > 0 {
            // 先挤掉已经办成的旧事，再挤最老的
            self.town_tasks.sort_by(|a, b| {
                a.done
                    .is_some()
                    .cmp(&b.done.is_some())
                    .then(a.posted_at.total_cmp(&b.posted_at))
            });
            self.town_tasks.drain(..excess);
        }
        Some(id)
    }

    /// 说过的全部（含已办成的），给页面和存档校验用。
    pub fn all_town_tasks(&self) -> &[TownTask] {
        &self.town_tasks
    }

    /// 还没办成的几件（给 host 挑要不要贴新的一条、给页面列）。
    pub fn pending_town_tasks(&self) -> impl Iterator<Item = &TownTask> {
        self.town_tasks.iter().filter(|t| t.done.is_none())
    }

    /// 这个人刚把东西放下：对上了哪件「送到某处」，就记成办成并向全镇说一声。
    pub(crate) fn settle_town_bring(
        &mut self,
        actor: &str,
        items: &[String],
        locale: &str,
        now: f64,
    ) {
        let hit = self.town_tasks.iter().position(|t| {
            t.done.is_none()
                && matches!(&t.task, Task::Bring { prop_id, locale: l }
                    if items.contains(prop_id) && l == locale)
        });
        self.finish_town_task(hit, actor, now);
    }

    /// 这个人刚做完一件事：对上了哪件镇上的事，就记成办成并向全镇说一声。
    pub(crate) fn settle_town_tasks(&mut self, actor: &str, verb: &str, locale: &str, now: f64) {
        let hit = self.town_tasks.iter().position(|t| {
            t.done.is_none()
                && matches!(&t.task, Task::Do { verb: v, locale: l } if v == verb && l == locale)
        });
        self.finish_town_task(hit, actor, now);
    }

    /// 认下第几条（`None` 表示没对上），向全镇说一声。
    fn finish_town_task(&mut self, hit: Option<usize>, actor: &str, now: f64) {
        let Some(index) = hit else {
            return;
        };
        let name = self
            .actors
            .get(actor)
            .map_or_else(|| actor.to_string(), |a| a.name.clone());
        let what = self.describe_task_only(&self.town_tasks[index].task);
        self.town_tasks[index].done = Some(Done {
            by: actor.to_string(),
            at: now,
        });
        self.announce(&format!("{name}把{what}做完了。"));
    }
}
