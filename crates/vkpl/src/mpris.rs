//! MPRIS adapter. D-Bus requests enter the same event loop as TUI commands.
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use tokio::sync::{mpsc, watch};
use zbus::{
    fdo,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

pub enum Command {
    Play,
    Pause,
    PlayPause,
    Stop,
    Next,
    Previous,
    Quit,
    Seek(i64),
    SetPosition(String, i64),
    Volume(f64),
    Shuffle(bool),
    Loop(String),
}
#[derive(Clone, PartialEq)]
pub struct State {
    pub status: String,
    pub track_id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: i64,
    pub position: i64,
    pub volume: f64,
    pub shuffle: bool,
    pub repeat: String,
    pub can_play: bool,
    pub can_seek: bool,
    pub can_pause: bool,
    pub can_next: bool,
    pub can_previous: bool,
    pub seek_serial: u64,
}
impl Default for State {
    fn default() -> Self {
        Self {
            status: "Stopped".into(),
            track_id: "/org/mpris/MediaPlayer2/TrackList/NoTrack".into(),
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            duration: 0,
            position: 0,
            volume: 0.6,
            shuffle: false,
            repeat: "None".into(),
            can_play: false,
            can_seek: false,
            can_pause: false,
            can_next: false,
            can_previous: false,
            seek_serial: 0,
        }
    }
}
impl State {
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let mut values = HashMap::new();
        let path =
            OwnedObjectPath::try_from(self.track_id.clone()).expect("generated MPRIS object path");
        values.insert(
            "mpris:trackid".into(),
            Value::from(path).try_into().unwrap(),
        );
        if !self.title.is_empty() {
            values.insert(
                "xesam:title".into(),
                Value::from(self.title.clone()).try_into().unwrap(),
            );
            values.insert(
                "xesam:artist".into(),
                Value::from(vec![self.artist.clone()]).try_into().unwrap(),
            );
            values.insert(
                "xesam:album".into(),
                Value::from(self.album.clone()).try_into().unwrap(),
            );
            values.insert("mpris:length".into(), OwnedValue::from(self.duration));
        }
        values
    }
}
struct Root {
    tx: mpsc::UnboundedSender<Command>,
}
#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {}
    fn quit(&self) {
        let _ = self.tx.send(Command::Quit);
    }
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn identity(&self) -> &str {
        "VKPL"
    }
    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        vec![]
    }
    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        vec![]
    }
}
struct Media {
    tx: mpsc::UnboundedSender<Command>,
    state: Arc<RwLock<State>>,
}
impl Media {
    fn send(&self, command: Command) -> fdo::Result<()> {
        self.tx
            .send(command)
            .map_err(|_| fdo::Error::Failed("VKPL is shutting down".into()))
    }
}
#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl Media {
    #[zbus(signal)]
    async fn seeked(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        position: i64,
    ) -> zbus::Result<()>;
    fn play(&self) -> fdo::Result<()> {
        self.send(Command::Play)
    }
    fn pause(&self) -> fdo::Result<()> {
        self.send(Command::Pause)
    }
    fn play_pause(&self) -> fdo::Result<()> {
        self.send(Command::PlayPause)
    }
    fn stop(&self) -> fdo::Result<()> {
        self.send(Command::Stop)
    }
    fn next(&self) -> fdo::Result<()> {
        self.send(Command::Next)
    }
    fn previous(&self) -> fdo::Result<()> {
        self.send(Command::Previous)
    }
    fn seek(&self, offset: i64) -> fdo::Result<()> {
        self.send(Command::Seek(offset))
    }
    fn set_position(&self, track_id: OwnedObjectPath, position: i64) -> fdo::Result<()> {
        self.send(Command::SetPosition(track_id.to_string(), position))
    }
    fn open_uri(&self, _uri: &str) -> fdo::Result<()> {
        Err(fdo::Error::NotSupported("Open tracks in VKPL".into()))
    }
    #[zbus(property)]
    fn playback_status(&self) -> String {
        self.state.read().unwrap().status.clone()
    }
    #[zbus(property)]
    fn loop_status(&self) -> String {
        self.state.read().unwrap().repeat.clone()
    }
    #[zbus(property)]
    fn set_loop_status(&self, value: &str) -> fdo::Result<()> {
        if !["None", "Track", "Playlist"].contains(&value) {
            return Err(fdo::Error::InvalidArgs("Invalid LoopStatus".into()));
        }
        self.send(Command::Loop(value.into()))?;
        self.state.write().unwrap().repeat = value.into();
        Ok(())
    }
    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn set_rate(&self, rate: f64) -> fdo::Result<()> {
        if rate == 0.0 {
            return self.send(Command::Pause);
        }
        if rate != 1.0 {
            return Err(fdo::Error::NotSupported(
                "Only rate 1.0 is supported".into(),
            ));
        }
        Ok(())
    }
    #[zbus(property)]
    fn shuffle(&self) -> bool {
        self.state.read().unwrap().shuffle
    }
    #[zbus(property)]
    fn set_shuffle(&self, value: bool) -> fdo::Result<()> {
        self.send(Command::Shuffle(value))?;
        self.state.write().unwrap().shuffle = value;
        Ok(())
    }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        self.state.read().unwrap().metadata()
    }
    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.state.read().unwrap().volume
    }
    #[zbus(property)]
    fn set_volume(&self, value: f64) -> fdo::Result<()> {
        if !value.is_finite() {
            return Err(fdo::Error::InvalidArgs("Volume must be finite".into()));
        }
        let value = value.clamp(0.0, 1.0);
        self.send(Command::Volume(value))?;
        self.state.write().unwrap().volume = value;
        Ok(())
    }
    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> i64 {
        self.state.read().unwrap().position
    }
    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        self.state.read().unwrap().can_next
    }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        self.state.read().unwrap().can_previous
    }
    #[zbus(property)]
    fn can_play(&self) -> bool {
        self.state.read().unwrap().can_play
    }
    #[zbus(property)]
    fn can_pause(&self) -> bool {
        self.state.read().unwrap().can_pause
    }
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.state.read().unwrap().can_seek
    }
    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}

pub struct Service {
    pub state: watch::Sender<State>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Service {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Service {
    pub async fn start(tx: mpsc::UnboundedSender<Command>) -> zbus::Result<Self> {
        let state = Arc::new(RwLock::new(State::default()));
        let connection = zbus::connection::Builder::session()?
            .name("org.mpris.MediaPlayer2.vkpl")?
            .serve_at("/org/mpris/MediaPlayer2", Root { tx: tx.clone() })?
            .serve_at(
                "/org/mpris/MediaPlayer2",
                Media {
                    tx,
                    state: state.clone(),
                },
            )?
            .build()
            .await?;
        let (sender, mut receiver) = watch::channel(State::default());
        let task = tokio::spawn(async move {
            let mut previous = State::default();
            while receiver.changed().await.is_ok() {
                let next = receiver.borrow_and_update().clone();
                *state.write().unwrap() = next.clone();
                let mut changed: HashMap<&str, Value<'_>> = HashMap::new();
                macro_rules! property {
                    ($field:ident, $name:literal) => {
                        if next.$field != previous.$field {
                            changed.insert($name, Value::from(next.$field.clone()));
                        }
                    };
                }
                property!(status, "PlaybackStatus");
                property!(volume, "Volume");
                property!(shuffle, "Shuffle");
                property!(repeat, "LoopStatus");
                property!(can_play, "CanPlay");
                property!(can_seek, "CanSeek");
                property!(can_pause, "CanPause");
                property!(can_next, "CanGoNext");
                property!(can_previous, "CanGoPrevious");
                if next.track_id != previous.track_id
                    || next.title != previous.title
                    || next.artist != previous.artist
                    || next.album != previous.album
                    || next.duration != previous.duration
                {
                    changed.insert("Metadata", Value::from(next.metadata()));
                }
                if !changed.is_empty() {
                    let _ = connection
                        .emit_signal(
                            None::<&str>,
                            "/org/mpris/MediaPlayer2",
                            "org.freedesktop.DBus.Properties",
                            "PropertiesChanged",
                            &("org.mpris.MediaPlayer2.Player", changed, Vec::<&str>::new()),
                        )
                        .await;
                }
                if next.seek_serial != previous.seek_serial {
                    let _ = connection
                        .emit_signal(
                            None::<&str>,
                            "/org/mpris/MediaPlayer2",
                            "org.mpris.MediaPlayer2.Player",
                            "Seeked",
                            &(next.position,),
                        )
                        .await;
                }
                previous = next;
            }
        });
        Ok(Self {
            state: sender,
            task,
        })
    }
}
