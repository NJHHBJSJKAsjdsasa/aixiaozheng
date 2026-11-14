//! 日程：模型排好的事到点由内核执行；到点还忙着就先等，等太久作废；被打断就整份作废。
//! 执行仍走 run_actions 的完整校验，和模型当场决定的动作同一条路。
use crate::common;
use kernel::World;
use protocol::ActionDraft;
use serde_json::json;

/// 甲地工作台、乙地干粮，和生存用例同一张图：走一趟就知道日程真的被执行了。
fn world() -> World {
    let topo = common::two_places(json!([
        {"id": "bench_a", "name": "甲地工作台", "locale": "loc_a",
         "verbs": [{"verb": "工作", "duration_minutes": 60, "effects": {"累": 1.0}}]},
        {"id": "rations_b", "name": "乙地干粮", "locale": "loc_b",
         "verbs": [{"verb": "吃干粮", "duration_minutes": 10, "effects": {"饿": -1.0}}]}
    ]));
    common::world_with(topo, 8.0 * 60.0, &[("ahe", "阿禾", "loc_a")])
}

fn go(locale: &str) -> ActionDraft {
    serde_json::from_value(json!({"intent": {"verb": "前往", "locale": locale}})).unwrap()
}

fn eat() -> ActionDraft {
    serde_json::from_value(json!({"intent": {"verb": "吃干粮"}, "prop_id": "rations_b"})).unwrap()
}

#[test]
fn a_step_runs_when_its_time_comes_and_not_before() {
    let mut w = world();
    let now = w.minutes();
    w.set_agenda("ahe", vec![(now + 30.0, vec![go("loc_b"), eat()])])
        .unwrap();
    assert!(w.take_due_agenda("ahe", now).is_none(), "没到点不该动他");
    let drafts = w
        .take_due_agenda("ahe", now + 30.0)
        .expect("到点该交给内核执行");
    assert_eq!(drafts.len(), 2);

    let revision = w.actor("ahe").unwrap().actions.revision;
    w.run_actions("ahe", "plan-1", revision, &drafts)
        .expect("日程也要过同一套校验");
    common::finish(&mut w, "ahe", "plan-1");
    assert_eq!(
        w.actor("ahe").unwrap().locus.locale,
        "loc_b",
        "这一件是走到乙地吃干粮"
    );
    assert!(w.agenda("ahe").unwrap().is_empty(), "做完就不在了");
}

#[test]
fn a_busy_resident_keeps_the_step_until_he_is_free_but_not_forever() {
    let mut w = world();
    let now = w.minutes();
    w.set_agenda(
        "ahe",
        vec![(now + 10.0, vec![go("loc_b")]), (now + 20.0, vec![eat()])],
    )
    .unwrap();
    // 到点时正在干一件 60 分钟的活：这一件留着，不打断手上的事
    let revision = w.actor("ahe").unwrap().actions.revision;
    let work: Vec<ActionDraft> = vec![serde_json::from_value(
        json!({"intent": {"verb": "工作", "duration_minutes": 60}, "prop_id": "bench_a"}),
    )
    .unwrap()];
    w.run_actions("ahe", "work-1", revision, &work).unwrap();
    w.step(10.0);
    assert!(
        w.take_due_agenda("ahe", now + 10.0).is_none(),
        "忙着就先等着"
    );
    assert_eq!(w.agenda("ahe").unwrap().len(), 2, "没做成就还在日程上");

    // 一直忙到超过容忍的迟到时间：这一件作废，下一件不受影响
    assert!(w.take_due_agenda("ahe", now + 10.0 + 91.0).is_none());
    assert_eq!(w.agenda("ahe").unwrap().len(), 1, "过期那件自己走掉");
    let next = w.agenda_next("ahe").unwrap();
    assert_eq!(next, now + 20.0, "剩下的是后一件");
}

#[test]
fn an_interruption_voids_the_rest_of_the_plan() {
    let mut w = world();
    let now = w.minutes();
    w.set_agenda(
        "ahe",
        vec![(now + 30.0, vec![go("loc_b")]), (now + 60.0, vec![eat()])],
    )
    .unwrap();
    w.interrupt_actions("ahe", "有人叫住他");
    assert!(
        w.agenda("ahe").unwrap().is_empty(),
        "计划赶不上变化：被打断就整份作废，交给模型重排"
    );
}

#[test]
fn impossible_plans_are_refused() {
    let mut w = world();
    let now = w.minutes();
    assert!(w.set_agenda("ahe", vec![]).is_err(), "一件都没排等于没排");
    assert!(
        w.set_agenda(
            "ahe",
            vec![
                (now + 10.0, vec![go("loc_b")]),
                (now + 10.0, vec![go("loc_b")])
            ]
        )
        .is_err(),
        "同一时刻排两件说不清先后"
    );
    assert!(
        w.set_agenda("ahe", vec![(now - 5.0, vec![go("loc_b")])])
            .is_err(),
        "已经过去的时刻排不了"
    );
    assert!(
        w.set_agenda("ahe", vec![(now + 10.0, vec![])]).is_err(),
        "一件里一组空动作"
    );
    let too_many: Vec<_> = (0..=kernel::schedule::AGENDA_STEPS_MAX)
        .map(|i| (now + 10.0 + i as f64, vec![go("loc_b")]))
        .collect();
    assert!(w.set_agenda("ahe", too_many).is_err(), "超过件数上限就拒绝");
    assert!(w.agenda("ahe").unwrap().is_empty(), "拒绝的不留下痕迹");
}
