use std::io;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tui_textarea::Input;

use agentwatch::app::{App, Tab, WORKFLOWS};
use agentwatch::event::{poll_event, Action};
use agentwatch::ui;

#[derive(Parser, Debug)]
#[command(name = "agentwatch", version, about = "Drive and observe agentic AI workflows.")]
struct Args {
    /// Start on a specific tab (1-9, 0). Default is Thread.
    #[arg(long)]
    tab: Option<u8>,
    /// Start with a specific workflow preset selected.
    /// One of: feature-build, bug-hunt, refactor, docs, review-only, oracle.
    #[arg(long)]
    workflow: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    if let Some(n) = args.tab {
        app.current_tab = Tab::from_index(n);
    }
    if let Some(name) = args.workflow {
        if let Some(idx) = WORKFLOWS.iter().position(|w| w.name == name) {
            app.workflow = idx;
        }
    }

    let res = run_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

fn run_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()> {
    let tick_rate = Duration::from_millis(33);
    loop {
        terminal.draw(|f| ui::render(f, app))?;

        if app.should_quit {
            break;
        }

        if let Some(action) = poll_event(
            tick_rate,
            app.current_tab,
            app.prompt_is_empty(),
            app.job_in_flight(),
            app.slash_mode(),
        )? {
            match action {
                Action::Quit => break,
                Action::SwitchTab(t) => app.current_tab = t,
                Action::NextTab => app.current_tab = app.current_tab.next(),
                Action::PrevTab => app.current_tab = app.current_tab.prev(),
                Action::Tick => app.tick(),
                Action::SelectionUp => app.move_selection(-1),
                Action::SelectionDown => app.move_selection(1),
                Action::Reload => app.reload_threads(),
                Action::SelectWorkflow(i) => {
                    if i < WORKFLOWS.len() {
                        app.workflow = i;
                    }
                }
                Action::PromptKey(k) => {
                    let _ = app.prompt.input(Input::from(k));
                    // Reset popup highlight whenever the prompt changes,
                    // so a previous selection doesn't out-of-bounds the
                    // new (possibly shorter) filter list.
                    app.slash_popup_idx = 0;
                }
                Action::PromptSubmit => app.submit_prompt(),
                Action::PromptCancel => app.clear_prompt(),
                Action::CancelJob => {
                    app.cancel_running_job();
                }
                Action::SlashPopupUp => app.slash_popup_up(),
                Action::SlashPopupDown => app.slash_popup_down(),
                Action::SlashPopupComplete => {
                    app.slash_complete();
                    app.slash_popup_idx = 0;
                }
                Action::ObserverActivate => match app.current_tab {
                    Tab::Sessions => {
                        app.resume_selected();
                    }
                    _ => {}
                },
            }
        }
    }
    Ok(())
}
