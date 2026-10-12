use super::*;

#[test]
fn stale_answers_cannot_answer_the_current_step() {
    let ui = Ui::new();
    ui.change(|v| v.step = 2);
    ui.answer
        .send(Reply {
            step: 1,
            answer: Answer::Skip,
        })
        .unwrap();
    ui.answer
        .send(Reply {
            step: 2,
            answer: Answer::Start,
        })
        .unwrap();
    assert!(matches!(ui.poll(), Some(Answer::Start)));
}

#[test]
fn an_answer_sent_as_soon_as_a_prompt_appears_is_kept() {
    let ui = Ui::new();
    let worker_ui = ui.clone();
    let worker = std::thread::spawn(move || {
        Person {
            ui: Some(worker_ui),
        }
        .ask(Phase::Card, "test", "")
    });
    let until = Instant::now() + Duration::from_secs(2);
    loop {
        let view = ui.view.lock().unwrap().clone();
        if view.step > 0 {
            ui.answer
                .send(Reply {
                    step: view.step,
                    answer: Answer::Start,
                })
                .unwrap();
            break;
        }
        assert!(Instant::now() < until);
        std::thread::yield_now();
    }
    assert!(matches!(worker.join().unwrap(), Ok(Answer::Start)));
}
