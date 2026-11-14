//! 往来账：聊过、给过、托过事，办成没办成，都从世界事件里累加；够分量才写进观察。
use crate::common;
use kernel::World;
use protocol::{
    Interaction, InteractionCommand as Command, InteractionKind as Kind, InteractionStatus,
};
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

fn command(w: &mut World, actor: &str, key: &str, cmd: Command) -> Interaction {
    w.interact(actor, key, w.actor(actor).unwrap().actions.revision, &cmd)
        .unwrap()
}

#[test]
fn a_finished_chat_shows_up_in_both_ledgers() {
    let mut w = world();
    let talk = command(
        &mut w,
        "r1",
        "talk",
        Command::Offer {
            partner_id: "r2".into(),
            interaction: Kind::Talk,
            message: "说两句".into(),
        },
    );
    command(
        &mut w,
        "r2",
        "reply",
        Command::Reply {
            interaction_id: talk.id.clone(),
            accept: true,
            message: "好".into(),
        },
    );
    command(
        &mut w,
        "r1",
        "leave",
        Command::Leave {
            interaction_id: talk.id.clone(),
            message: "先走了".into(),
        },
    );
    assert_eq!(
        w.interactions().of("r1", "talk").unwrap().status,
        InteractionStatus::Succeeded
    );
    let a = &w.actor("r1").unwrap().relations["r2"];
    let b = &w.actor("r2").unwrap().relations["r1"];
    assert_eq!(a.talks, 1);
    assert_eq!(b.talks, 1);
    assert!(a.note().unwrap().contains("聊过 1 次"));
}

#[test]
fn a_finished_ask_is_a_debt_on_both_sides() {
    let mut w = world();
    let at = w.minutes() + 120.0;
    let ask = command(
        &mut w,
        "r1",
        "ask",
        Command::Offer {
            partner_id: "r2".into(),
            interaction: Kind::Ask {
                task: protocol::Task::Do {
                    verb: "制作器物".into(),
                    locale: "loc_a".into(),
                },
                at,
            },
            message: "帮我做一件".into(),
        },
    );
    command(
        &mut w,
        "r2",
        "reply",
        Command::Reply {
            interaction_id: ask.id.clone(),
            accept: true,
            message: "好".into(),
        },
    );
    common::run(
        &mut w,
        "r2",
        "work",
        json!({"intent":{"verb":"制作器物"},"prop_id":"bench"}),
    )
    .unwrap();
    common::finish(&mut w, "r2", "work");

    // 托的人记得「他给我办成了」，做的人记得「他托我，我办成了」
    let initiator = &w.actor("r1").unwrap().relations["r2"];
    assert_eq!(initiator.done_by, 1);
    assert!(initiator.note().unwrap().contains("你托过他 1 件事"));
    let partner = &w.actor("r2").unwrap().relations["r1"];
    assert_eq!(partner.done_for, 1);
    assert!(partner.note().unwrap().contains("他托过你 1 件事"));
}
