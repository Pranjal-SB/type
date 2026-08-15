use typ_app::prompt::{Prompt, PromptKind};

#[test]
fn a_new_prompt_starts_empty_and_knows_what_it_is_for() {
    let prompt = Prompt::new(PromptKind::Search);
    assert_eq!(prompt.input(), "");
    assert_eq!(prompt.kind(), PromptKind::Search);
}

#[test]
fn typing_accumulates_into_the_input() {
    let mut prompt = Prompt::new(PromptKind::Search);
    prompt.insert_char('f');
    prompt.insert_char('n');
    assert_eq!(prompt.input(), "fn");
}

#[test]
fn backspace_removes_a_whole_grapheme() {
    let mut prompt = Prompt::new(PromptKind::Search);
    for c in "日本".chars() {
        prompt.insert_char(c);
    }
    prompt.delete_backward();
    assert_eq!(prompt.input(), "日");
}

#[test]
fn backspace_on_an_empty_prompt_is_harmless() {
    let mut prompt = Prompt::new(PromptKind::Search);
    prompt.delete_backward();
    assert_eq!(prompt.input(), "");
}

#[test]
fn the_label_says_which_prompt_this_is() {
    assert_eq!(Prompt::new(PromptKind::Search).label(), "Search:");
    assert_eq!(Prompt::new(PromptKind::Replace).label(), "Replace with:");
}

#[test]
fn a_replace_prompt_asks_the_second_question_in_place() {
    let mut prompt = Prompt::new(PromptKind::Search);
    prompt.become_replace_after_needle();
    assert!(prompt.is_replace_flow());

    // Two statements, not one: `prompt.set_pending_needle(prompt.take_input())`
    // needs a mutable borrow inside a mutable borrow, which two-phase borrows
    // do not permit.
    let needle = prompt.take_input();
    prompt.set_pending_needle(needle);

    prompt.become_replace();
    assert_eq!(prompt.kind(), PromptKind::Replace);
    assert_eq!(prompt.label(), "Replace with:");
    assert!(
        !prompt.is_replace_flow(),
        "the flow is spent once the needle is banked"
    );
}

#[test]
fn taking_the_input_leaves_the_prompt_empty() {
    let mut prompt = Prompt::new(PromptKind::Search);
    prompt.insert_char('a');
    assert_eq!(prompt.take_input(), "a");
    assert_eq!(prompt.input(), "");
}
