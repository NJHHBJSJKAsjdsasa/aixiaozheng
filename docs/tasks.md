# 人物与任务机制

这份文档讲清楚一件事：**镇上的「事情」有几种、谁认账、办成没办成记在哪**。
代码分散在 `kernel/src/{schedule,relations,town_tasks}.rs` 与 `kernel/src/interactions/{asks,appointments}.rs`，
这里只做索引和规则说明，改之前先读对应模块的头注释。

## 四种来源

| 来源 | 谁发起 | 什么时候出现 | 代码 |
| --- | --- | --- | --- |
| 自己定的目标 | 居民自己 | 模型随时 `set_goal` | `agent/src/tools/goals.rs` |
| 请托（做某件事） | 另一个居民或玩家化身 | 交谈里 `propose`、或直接 `offer_invitation` | `interactions/asks.rs` |
| 请托（交付物件） | 同上 | 同上，`task.kind = deliver` | 同上 + `commands.rs` 的赠予结算 |
| 镇上的事（无主） | 世界 | host 每天说一件，谁先做完算谁的；可以是「在某处做某事」或「把某件东西送到某处放下」 | `town_tasks.rs` + `host/src/runtime.rs::post_town_task` |
| 约定（见面） | 居民 | 交谈里 `propose` 的 `appointment` | `interactions/appointments.rs` |

镇上的事不是地图里每件能做的事都拿来说：`post_town_task` 先筛掉吃喝睡（身体自己管）、
走路的动作、只动嘴动腿的流程活（`接受检查` / `购买…` / `查看…` 等前缀）和不足 5 分钟的小事，
剩下的按日期轮着挑一处（规则见 `host/src/runtime.rs::worth_posting`）。图书馆的「整理借还」
「归还书籍」是正经活，不算流程。

前四种都会落到 `Goal`（`agent/src/session.rs`）上：接受者得一条 `ask-<互动编号>` 或 `town-<编号>`，
自己定的用模型给的 `id`。**目标只负责提醒，不负责认定。**

## 谁认账：世界按动作认，不听自报

- **做某件事**（`Task::Do { verb, locale }`）：动作组结算完成后（`simulation.rs::complete_current`
  → `settle_asks` / `settle_town_tasks`），比对这次动作的动词与处所。对上才算。
- **交付给人**（`Task::Deliver { prop_id, to }`）：赠予互动结算完成后（`commands.rs` 的 Give 分支
  → `settle_delivery`），比对这次赠予的物品与收件人。东西真到了对方背包才算。
- **送到某处**（`Task::Bring { prop_id, locale }`，只用于镇上的事）：背包放下动作结算后
  （`inventory/commands.rs::finish_inventory` 的 Drop 分支 → `settle_town_bring`），比对
  放下的物品与当时的处所。谁送都算，落在那儿就算办成。
- **模型自报完成**（`set_goal` 的 `completed`）需要世界证据：`completion_evidence` 会去翻
  互动状态或动作组结果，编不出来。
- **世界认下之后，目标自己结掉**（`runtime.rs::complete_goals`）：`ask-<互动>` 看互动是不是
  `Succeeded`、`town-<编号>` 看那条事的 `done.by` 是不是自己；自己盯着某次互动的目标也照此结。
  办成了还挂着、到点反被判成没做成，是这套机制最容易出的错。
- **到点时先看世界再判失信**：自己定的动作目标过了约定时刻，若 `completion_evidence` 已能
  证明做成，就记完成，不记失败。请托与镇上的事不走这条（以世界那条记录为准，迟了就是迟了）。

## 时限

| 字段 | 含义 | 到期怎么处理（动作已由世界证明做成时，提醒不发、到点记完成） |
| --- | --- | --- |
| `Goal::due_clock`（`D2 14:00`） | 目标自己的约定时刻 | 剩 60 分钟内提醒一次；过点转 `Failed`，原因写「约定时刻到了还没做成」，并留一句懊恼 |
| `InteractionKind::Ask { at }` | 请托的截止时刻 | 提前 30 分钟提醒双方；过点转 `Expired`，双方各记一笔「没办成」 |
| `Task` 无期限 | 镇上的事 | 挂满一天催一次（「昨天说的…还没人做」），挂满两天撤下（「先不提了」）；同时最多挂 4 条 |

`set_goal` 不接受已经过去的时刻（收尾时抄上的旧时刻除外）。

## 账本（`relations.rs`）

每人一份 `Actor::relations`：`BTreeMap<对方编号, Relation>`，随 `world.json` 存，
每人只留最近来往的 8 份。

| 字段 | 记什么 |
| --- | --- |
| `talks` | 交谈结束（`end_chat`）时双方各 +1 |
| `gifts` | 给出为正、收到为负（赠予结算后） |
| `done_for` / `missed_for` | 他托我，我办成 / 没办成 |
| `done_by` / `missed_by` | 我托他，他办成 / 没办成 |
| `refused_for` / `refused_by` | 他求我我推了 / 我求他被推了 |
| `last` | 最近一次打交道的时刻（裁剪时按它挤掉最久没来往的） |

观察里只给分量最重的 3 句（`Relation::note`），托付类最重、其次是东西、最后才是闲聊。

## 模型看得见什么

| 观察字段 | 内容 |
| --- | --- |
| `goals`（题面的 goals 栏） | 自己的目标：条件、状态、承诺、期限 |
| `agenda` | 自己排的日程：还剩几件、下一件多久后做什么 |
| `town_tasks` | 镇上还没人做完的事：id、做什么、说出来多久了 |
| `relations` | 往来账最要紧的 3 句 |
| `interactions` | 手上没完的互动：请托、约定、同行、交谈 |

相关工具：`set_goal`、`plan`、`take_task`、`offer_invitation`、`reply_invitation`、`interact`。

## 存档字段

| 字段 | 位置 | 兼容方式 |
| --- | --- | --- |
| `agenda` | `Actor` | 空时不写出 |
| `relations` | `Actor` | 空时不写出 |
| `town_tasks` | `WorldSnapshot` | `skip_serializing_if = "Vec::is_empty"`（空时不能写出，否则 v5 旧档迁移回归会挂） |
| `goals` | `Session`（resident agent 档） | 新字段 `#[serde(default)]` |

## 扩展点

1. **自动派「送到某处」的活**：内核已支持（`Task::Bring`，放下即认），但 host 还没派——
   因为要挪动世界里的道具，得先想清楚它原本的用途（药箱被送走，诊所就没药了）。
   做法大概是：只挑"成堆的消耗品"（道具定义里 `stack_limit > 1` 且不影响做饭、治病），
   或者先给地图加"仓库/信箱"这类专用收件道具。
2. **玩家入口**：`InteractionKind::Ask` 已经能被 `offer_invitation` 发出，缺的是前端交谈面板的
   表单和一张"在办的事"卡片。
3. **让账本影响行为**：现在账只被读，没有被世界用来排序（比如镇上的事优先说给欠人情的人）。
   做的时候注意别替模型判断——"该找谁"应该是模型看了账自己决定。
