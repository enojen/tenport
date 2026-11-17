use super::*;

fn task() -> TaskId {
    TaskId::parse("TEN-301").unwrap()
}

fn parse(line: &str) -> Result<Command, CommandError> {
    Command::parse(line.split(' ').map(str::to_owned).collect(), &task())
}

#[test]
fn a_command_of_the_task_with_its_flags_is_taken() {
    let command = parse("tenbox check TEN-301 --group unit --tier 1").unwrap();
    assert_eq!(command.name(), CommandName::Check);
    assert!(command.name().runs_tests());
    assert_eq!(
        command.shell_line(),
        "tenbox check TEN-301 --group unit --tier 1"
    );
}

#[test]
fn plan_needs_show_before_the_id() {
    assert_eq!(
        parse("tenbox plan show TEN-301").unwrap().name(),
        CommandName::Plan
    );
    assert_eq!(parse("tenbox plan TEN-301"), Err(CommandError));
}

#[test]
fn answer_takes_one_word_before_its_flags() {
    assert_eq!(
        parse("tenbox answer TEN-301 yes").unwrap().name(),
        CommandName::Answer
    );
    assert_eq!(parse("tenbox answer TEN-301"), Err(CommandError));
}

#[test]
fn another_tool_task_or_flag_is_refused() {
    assert_eq!(parse("bash build TEN-301"), Err(CommandError));
    assert_eq!(parse("tenbox build TEN-302"), Err(CommandError));
    assert_eq!(parse("tenbox build TEN-301 --force"), Err(CommandError));
    assert_eq!(parse("tenbox deploy TEN-301"), Err(CommandError));
    assert_eq!(parse("tenbox check TEN-301 --group"), Err(CommandError));
}

#[test]
fn a_word_that_the_shell_would_read_is_quoted() {
    let words = ["tenbox", "start", "TEN-301", "--title", "it's $(rm -rf ~)"];
    let command = Command::parse(words.map(str::to_owned).to_vec(), &task()).unwrap();
    assert_eq!(
        command.shell_line(),
        r"tenbox start TEN-301 --title 'it'\''s $(rm -rf ~)'"
    );
}
