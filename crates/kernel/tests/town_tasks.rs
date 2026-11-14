//! 镇上的事：世界自己说出一条活，全镇都听见；谁先做完算谁的，做完再向全镇说一声。
use crate::common;
use kernel::World;
use serde_json::json;

fn world() -> World {
    let topo = common::two_places(json!([
        {"id":"bench","name":"工作台","locale":"loc_a","verbs":[{"verb":"制作器物","duration_minutes":2}]}
    ]));
    let mut w = common::world_with(
        topo,
        600.0,
        &[("r1", "阿树", "loc_a"), ("r2", "小禾", "loc_a")],
    );
    for id in ["r1", "r2"] {
        w.enable_agent(id).unwrap();
    }
    w
}

fn heard(w: &World, actor: &str, text: &str) -> bool {
    w.actor(actor)
        .unwrap()
        .journal
        .events_after(0)
        .iter()
        .any(|e| e.text.contains(text))
}

#[test]
fn a_town_task_is_heard_by_everyone_and_settled_once() {
    let mut w = world();
    let posted = w
        .post_town_task(protocol::Task::Do {
            verb: "制作器物".into(),
            locale: "loc_a".into(),
        })
        .expect("该说得出去");
    assert!(!posted.is_empty());
    assert_eq!(w.pending_town_tasks().count(), 1);
    assert!(heard(&w, "r1", "镇上的事"), "谁在场都听见");
    assert!(heard(&w, "r2", "镇上的事"));

    // 同一件事还挂着时不再说第二遍
    assert!(
        w.post_town_task(protocol::Task::Do {
            verb: "制作器物".into(),
            locale: "loc_a".into(),
        })
        .is_none(),
        "同一件活不重复说"
    );

    common::run(
        &mut w,
        "r2",
        "work",
        json!({"intent":{"verb":"制作器物"},"prop_id":"bench"}),
    )
    .unwrap();
    common::finish(&mut w, "r2", "work");
    assert_eq!(w.pending_town_tasks().count(), 0, "有人做完了");
    assert!(heard(&w, "r1", "做完了"), "全镇都听说办成了");
}

#[test]
fn settled_tasks_survive_saving_and_loading() {
    let mut w = world();
    w.post_town_task(protocol::Task::Do {
        verb: "制作器物".into(),
        locale: "loc_a".into(),
    })
    .unwrap();
    common::run(
        &mut w,
        "r2",
        "work",
        json!({"intent":{"verb":"制作器物"},"prop_id":"bench"}),
    )
    .unwrap();
    common::finish(&mut w, "r2", "work");
    let restored = common::restore(&w, world());
    assert_eq!(restored.pending_town_tasks().count(), 0);
    assert_eq!(
        restored.all_town_tasks().len(),
        1,
        "办成的那条留着，不会被重复说起"
    );
}

/// 送到某处也能是镇上的事：东西放下了才算办成。
#[test]
fn bringing_something_to_a_place_settles_the_task() {
    let mut w = world();
    let parcel: kernel::props::Definition = serde_json::from_value(json!({
        "id":"player:parcel","version":1,"name":"包裹","portable":true,"stack_limit":5,"actions":[]
    }))
    .unwrap();
    w.place_prop(
        serde_json::from_value(json!({
            "instance":{"id":"p1","definition":parcel.reference(),"locale":"loc_a"},
            "definition":parcel
        }))
        .unwrap(),
    )
    .unwrap();
    w.post_town_task(protocol::Task::Bring {
        prop_id: "p1".into(),
        locale: "loc_b".into(),
    })
    .unwrap();
    assert_eq!(w.pending_town_tasks().count(), 1);

    common::run(
        &mut w,
        "r2",
        "pick",
        json!({"intent":{"verb":"拾取"},"inventory":{"operation":"pick_up","items":["p1"]}}),
    )
    .unwrap();
    common::finish(&mut w, "r2", "pick");
    common::run(
        &mut w,
        "r2",
        "go",
        json!({"intent":{"verb":"前往","locale":"loc_b"}}),
    )
    .unwrap();
    common::finish(&mut w, "r2", "go");
    common::run(
        &mut w,
        "r2",
        "drop",
        json!({"intent":{"verb":"放下"},"inventory":{"operation":"drop","items":["p1"]}}),
    )
    .unwrap();
    common::finish(&mut w, "r2", "drop");
    assert_eq!(w.pending_town_tasks().count(), 0, "放下了才算送到");
    assert!(heard(&w, "r1", "做完了"));
}

/// 说出去的事没人做：一天催一次，两天撤下来，别一直挂着当摆设。
#[test]
fn an_ignored_task_is_nagged_once_then_dropped() {
    let mut w = world();
    w.post_town_task(protocol::Task::Do {
        verb: "制作器物".into(),
        locale: "loc_a".into(),
    })
    .unwrap();
    // 跨过一天：说一句「还没人做」
    w.step(1441.0);
    assert!(heard(&w, "r1", "还没人做"), "一天没人做要催一句");
    assert_eq!(w.pending_town_tasks().count(), 1, "催归催，还挂着");
    // 再跨一天：撤下来
    w.step(1441.0);
    assert_eq!(w.all_town_tasks().len(), 0, "两天没人做就撤了");
}
