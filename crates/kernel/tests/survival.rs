//! 生存自动机：饿、渴、累到紧急时由内核直接安排下一步；普通状态、忙着的、被玩家接管的不打扰。
//! 安排出来的草案走 run_actions 的完整校验与执行，和模型动作同一条路，不另开旁路。
use crate::common;
use kernel::props::Need;
use kernel::survival;
use kernel::World;
use serde_json::json;

/// 甲地一个村民，乙地一袋干粮：饿了要走到乙地再吃。
fn world_with_food() -> World {
    let topo = common::two_places(json!([
        {"id": "bench_a", "name": "甲地工作台", "locale": "loc_a",
         "verbs": [{"verb": "工作", "duration_minutes": 60, "effects": {"累": 1.0}}]},
        {"id": "rations_b", "name": "乙地干粮", "locale": "loc_b",
         "verbs": [{"verb": "吃干粮", "duration_minutes": 10, "effects": {"饿": -1.0}}]}
    ]));
    common::world_with(topo, 8.0 * 60.0, &[("ahe", "阿禾", "loc_a")])
}

/// 推到饱腹跌破「重新考虑」线，但还没到失能。
fn starve(w: &mut World, actor: &str) {
    let ring = w.actor(actor).unwrap().needs.tuning.ring;
    drain(w, actor, ring - 0.5);
}

/// 空着肚子走到饱腹跌破 `target`。
fn drain(w: &mut World, actor: &str, target: f64) {
    for _ in 0..2000 {
        if w.actor(actor).unwrap().needs.track(Need::Fullness).value < target {
            return;
        }
        w.step(2.0);
    }
    panic!("没能让 {actor} 饿到 {target}");
}

#[test]
fn hungry_resident_walks_to_food_on_its_own() {
    let mut w = world_with_food();
    starve(&mut w, "ahe");
    let plan = survival::takeover(&w, "ahe").expect("饿了该自己安排");
    assert_eq!(plan.drafts.len(), 2, "不在乙地：一项前往加一项吃");
    assert_eq!(plan.drafts[0].intent.verb, "前往");
    assert_eq!(plan.drafts[0].intent.locale.as_deref(), Some("loc_b"));
    assert_eq!(plan.drafts[1].intent.verb, "吃干粮");
    assert_eq!(plan.drafts[1].prop_id.as_deref(), Some("rations_b"));
    assert!(
        plan.thought.starts_with("很饿") && plan.thought.contains("吃干粮"),
        "要带上这一档的身体说明与要做的事：{}",
        plan.thought
    );
}

#[test]
fn already_there_skips_the_walk() {
    let topo = common::two_places(json!([
        {"id": "rations_a", "name": "甲地干粮", "locale": "loc_a",
         "verbs": [{"verb": "吃干粮", "duration_minutes": 10, "effects": {"饿": -1.0}}]}
    ]));
    let mut w = common::world_with(topo, 8.0 * 60.0, &[("ahe", "阿禾", "loc_a")]);
    starve(&mut w, "ahe");
    let plan = survival::takeover(&w, "ahe").expect("饿了该自己安排");
    assert_eq!(plan.drafts.len(), 1, "已经在甲地：只吃，不走");
    assert_eq!(plan.drafts[0].intent.verb, "吃干粮");
}

#[test]
fn comfortable_resident_is_left_to_the_model() {
    let w = world_with_food();
    assert!(
        survival::takeover(&w, "ahe").is_none(),
        "不饿不累不渴时不插手"
    );
}

#[test]
fn arranged_group_runs_to_the_end_and_fills_the_belly() {
    let mut w = world_with_food();
    starve(&mut w, "ahe");
    let before = w.actor("ahe").unwrap().needs.track(Need::Fullness).value;
    let plan = survival::takeover(&w, "ahe").expect("饿了该自己安排");
    let revision = w.actor("ahe").unwrap().actions.revision;
    w.run_actions("ahe", "survival-1", revision, &plan.drafts)
        .expect("安排要能通过内核校验");
    common::finish(&mut w, "ahe", "survival-1");
    let after = w.actor("ahe").unwrap().needs.track(Need::Fullness).value;
    assert!(after > before, "吃完要回升：吃前 {before}，吃后 {after}");
    assert_eq!(
        w.actor("ahe").unwrap().locus.locale,
        "loc_b",
        "要走到干粮所在的乙地"
    );
}

#[test]
fn collapse_is_marked_pressing() {
    let mut w = world_with_food();
    // 一路饿到崩溃线要一整天，水分会先见底把人拖到失能；这里直接把饱腹放到崩溃线下。
    w.set_body_values("ahe", Some(12.0), None, Some(80.0), None)
        .unwrap();
    let plan = survival::takeover(&w, "ahe").expect("失控档更要安排");
    assert!(plan.pressing, "失控档要标出来，调用方才知道不必等模型");
    assert!(
        plan.thought.starts_with("身体撑不住了"),
        "失控档那句换短的：{}",
        plan.thought
    );
}

#[test]
fn busy_resident_is_not_interrupted() {
    let mut w = world_with_food();
    starve(&mut w, "ahe");
    // 先让他自己走一趟，动作组在途时不该再塞一份安排
    let plan = survival::takeover(&w, "ahe").expect("饿了该自己安排");
    let revision = w.actor("ahe").unwrap().actions.revision;
    w.run_actions("ahe", "survival-1", revision, &plan.drafts)
        .unwrap();
    assert!(
        survival::takeover(&w, "ahe").is_none(),
        "有安排在手时不重复安排"
    );
}
