use crossbeam_channel::Sender;
use ksni::{blocking::TrayMethods, menu::StandardItem, MenuItem, Tray};

pub enum TrayAction {
    ShowWindow,
    Rescan,
    Quit,
}

pub struct DocSearchTray {
    pub sender: Sender<TrayAction>,
}

impl Tray for DocSearchTray {
    fn id(&self) -> String {
        "docsearch".into()
    }

    fn title(&self) -> String {
        "DocSearch".into()
    }

    fn icon_name(&self) -> String {
        "system-search".into()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.sender.send(TrayAction::ShowWindow);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let tx_open = self.sender.clone();
        let tx_rescan = self.sender.clone();
        let tx_quit = self.sender.clone();

        vec![
            StandardItem {
                label: "🔍 Open DocSearch".into(),
                activate: Box::new(move |_| {
                    let _ = tx_open.send(TrayAction::ShowWindow);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "🔄 Re-scan Folders".into(),
                activate: Box::new(move |_| {
                    let _ = tx_rescan.send(TrayAction::Rescan);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "❌ Quit".into(),
                activate: Box::new(move |_| {
                    let _ = tx_quit.send(TrayAction::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

pub fn spawn_tray(sender: Sender<TrayAction>) {
    let tray = DocSearchTray { sender };
    let _ = tray.disable_dbus_name(true).spawn();
}
