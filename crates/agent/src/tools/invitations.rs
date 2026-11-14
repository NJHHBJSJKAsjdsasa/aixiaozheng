//! 发邀请与回应邀请的工具，各用独立参数结构。
use super::*;
use crate::session::{Goal, GoalCondition, GoalStatus, InvitationReplyFailure};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Offer {
    partner_id: String,
    interaction: protocol::InteractionKind,
    message: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    interaction_id: String,
    accept: bool,
    message: String,
}

pub(super) fn offer(ctx: &mut ToolContext, call: &ToolCall) -> Result<ToolOutcome, ToolError> {
    let mut args = call.arguments.clone();
    let partner = args["partner_id"].as_str().map(str::to_owned);
    resolve_leader(
        ctx.world,
        &ctx.session.actor_id,
        partner.as_deref(),
        &mut args["interaction"],
    );
    resolve_appointment(ctx.world, &mut args["interaction"])?;
    expand_gift(ctx.world, &mut args["interaction"])?;
    let a: Offer = decode(&args)?;
    execute_interaction(
        ctx,
        call,
        InteractionCommand::Offer {
            partner_id: a.partner_id,
            interaction: a.interaction,
            message: a.message,
        },
    )
}

pub(super) fn reply(ctx: &mut ToolContext, call: &ToolCall) -> Result<ToolOutcome, ToolError> {
    let a: Reply = match decode(&call.arguments) {
        Ok(a) => a,
        Err(error) => {
            let ids: Vec<_> = ctx
                .world
                .interactions()
                .for_actor(&ctx.session.actor_id)
                .into_iter()
                .filter(|i| {
                    i.partner == ctx.session.actor_id && i.status == InteractionStatus::Offered
                })
                .map(|i| i.id.clone())
                .collect();
            let attempts = ctx
                .session
                .invitation_reply_failure
                .as_ref()
                .filter(|f| f.invitation_ids == ids)
                .map_or(1, |f| f.attempts.saturating_add(1));
            ctx.session.invitation_reply_failure = Some(InvitationReplyFailure {
                invitation_ids: ids.clone(),
                attempts,
            });
            let message = format!("{error}；请修正后调用 reply_invitation，只填 interaction_id、accept、message。当前可回应的邀请编号：{}。accept 必须由你明确选择 true 或 false；不要原样重发错误参数。", json!(ids));
            return Err(ToolError::new(ToolErrorKind::InvalidArguments, message));
        }
    };
    let (id, accept) = (a.interaction_id.clone(), a.accept);
    let result = execute_interaction(
        ctx,
        call,
        InteractionCommand::Reply {
            interaction_id: a.interaction_id,
            accept: a.accept,
            message: a.message,
        },
    );
    if result.is_ok() {
        ctx.session.invitation_reply_failure = None;
        if accept {
            remember_ask(ctx, &id);
        }
    }
    result
}

/// 答应下来的请托记成自己的目标（编号 `ask-<互动>`）：题面里有它，到期结算也管它。
/// 完成仍由世界按动作认，这条目标只是让自己别忘了。
fn remember_ask(ctx: &mut ToolContext, id: &str) {
    let Some(item) = ctx
        .world
        .interactions()
        .of(&ctx.session.actor_id, id)
        .cloned()
    else {
        return;
    };
    if item.accepted != Some(true) {
        return;
    }
    let protocol::InteractionKind::Ask { task, at } = &item.kind else {
        return;
    };
    let protocol::Task::Do { verb, locale } = task;
    let id = format!("ask-{}", item.id);
    if ctx.session.goals.contains_key(&id) {
        return;
    }
    let giver = ctx
        .world
        .actor(&item.initiator)
        .map_or_else(|| item.initiator.clone(), |a| a.name.clone());
    let clock = kernel::clock_string(*at);
    let now = ctx.world.minutes();
    ctx.session.goals.insert(
        id.clone(),
        Goal {
            id,
            title: format!(
                "{giver}托的事：去{}做「{verb}」",
                ctx.world.topo.locale_name(locale)
            ),
            condition: GoalCondition::Action {
                verb: verb.clone(),
                locale: locale.clone(),
            },
            status: GoalStatus::Active,
            next_steps: vec![],
            commitment: item.message.clone(),
            reason: format!("{giver}托的，要在 {clock} 前办完"),
            created_at: now,
            updated_at: now,
            warned: false,
            due_clock: Some(clock),
            evidence: vec![],
        },
    );
}
