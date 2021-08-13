use serenity::framework::standard::macros::{command, group};
use serenity::framework::standard::CommandResult;

#[command]
async fn adminlog() -> CommandResult {
    Ok(())
}

#[command]
async fn addlog() -> CommandResult {
    Ok(())
}

#[command]
async fn editlog() -> CommandResult {
    Ok(())
}

#[command]
async fn removelog() -> CommandResult {
    Ok(())
}

#[group]
#[commands(adminlog, addlog, editlog, removelog)]
#[required_permissions("Administrator")]
struct Administrative;
