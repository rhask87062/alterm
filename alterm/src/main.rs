use iced::{Element, Theme};

fn main() -> iced::Result {
    env_logger::init();

    iced::application(Altermative::new, Altermative::update, Altermative::view)
        .title("Altermative")
        .theme(Theme::Dark)
        .window_size((900.0, 600.0))
        .run()
}

#[derive(Default)]
struct Altermative;

#[derive(Debug, Clone)]
enum Message {}

impl Altermative {
    fn new() -> Self {
        Altermative
    }

    fn update(&mut self, _message: Message) {}

    fn view(&self) -> Element<'_, Message> {
        iced::widget::container(
            iced::widget::text("Altermative — terminal coming soon").size(20),
        )
        .width(iced::Length::Fill)
        .height(iced::Length::Fill)
        .center_x(iced::Length::Fill)
        .center_y(iced::Length::Fill)
        .into()
    }
}
