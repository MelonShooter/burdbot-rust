use std::collections::HashMap;
use std::error::Error as StdError;
use std::fmt::Debug;
use std::sync::Arc;
use std::sync::Mutex;

use lazy_static::lazy_static;
use petgraph::algo;
use petgraph::graph::NodeIndex;
use petgraph::graph::UnGraph;
use regex::Regex;
use reqwest::Client;
use reqwest::Error;
use scraper::ElementRef;
use scraper::Html;
use scraper::Selector;
use std::fmt::Display;
use strum::EnumProperty;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use strum_macros::EnumProperty;
use strum_macros::EnumString;

use serenity::client::Context;
use serenity::framework::standard::Args;
use serenity::model::channel::Message;

use crate::commands::error_util::error::NotEnoughArgumentsError;
use crate::commands::{self, error_util, ArgumentInfo};

lazy_static! {
    static ref FORVO_CLIENT: Client = Client::new();
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, EnumIter, EnumString, EnumProperty)]
enum Country {
    #[strum(serialize = "🇦🇷", serialize = "Argentina", props(flag = "🇦🇷", index = "0"))]
    Argentina,
    #[strum(serialize = "🇺🇾", serialize = "Uruguay", props(flag = "🇺🇾", index = "1"))]
    Uruguay,
    #[strum(serialize = "🇨🇱", serialize = "Chile", props(flag = "🇨🇱", index = "2"))]
    Chile,
    #[strum(serialize = "🇵🇪", serialize = "Peru", props(flag = "🇵🇪", index = "3"))]
    Peru,
    #[strum(serialize = "🇧🇴", serialize = "Bolivia", props(flag = "🇧🇴", index = "4"))]
    Bolivia,
    #[strum(serialize = "🇵🇾", serialize = "Paraguay", props(flag = "🇵🇾", index = "5"))]
    Paraguay,
    #[strum(serialize = "🇪🇨", serialize = "Ecuador", props(flag = "🇪🇨", index = "6"))]
    Ecuador,
    #[strum(serialize = "🇨🇴", serialize = "Colombia", props(flag = "🇨🇴", index = "7"))]
    Colombia,
    #[strum(serialize = "🇻🇪", serialize = "Venezuela", props(flag = "🇻🇪", index = "8"))]
    Venezuela,
    #[strum(serialize = "🇵🇦", serialize = "Panama", props(flag = "🇵🇦", index = "9"))]
    Panama,
    #[strum(serialize = "🇨🇷", serialize = "Costa Rica", props(flag = "🇨🇷", index = "10"))]
    CostaRica,
    #[strum(serialize = "🇸🇻", serialize = "El Salvador", props(flag = "🇸🇻", index = "11"))]
    ElSalvador,
    #[strum(serialize = "🇳🇮", serialize = "Nicaragua", props(flag = "🇳🇮", index = "12"))]
    Nicaragua,
    #[strum(serialize = "🇬🇹", serialize = "Guatemala", props(flag = "🇬🇹", index = "13"))]
    Guatemala,
    #[strum(serialize = "🇭🇳", serialize = "Honduras", props(flag = "🇭🇳", index = "14"))]
    Honduras,
    #[strum(serialize = "🇲🇽", serialize = "Mexico", props(flag = "🇲🇽", index = "15"))]
    Mexico,
    #[strum(serialize = "🇨🇺", serialize = "Cuba", props(flag = "🇨🇺", index = "16"))]
    Cuba,
    #[strum(serialize = "🇩🇴", serialize = "Dominican Republic", props(flag = "🇩🇴", index = "17"))]
    DominicanRepublic,
    #[strum(serialize = "🇪🇸", serialize = "Spain", props(flag = "🇪🇸", index = "18"))]
    Spain,

    // UNITED STATES MUST BE THE FIRST ENGLISH SPEAKING COUNTRY BY INDEX IN THIS LIST.
    #[strum(serialize = "🇺🇸", serialize = "United States", props(flag = "🇺🇸", index = "19"))]
    UnitedStates,
    #[strum(serialize = "🇨🇦", serialize = "Canada", props(flag = "🇨🇦", index = "20"))]
    Canada,
    #[strum(serialize = "🇬🇧", serialize = "United Kingdom", props(flag = "🇬🇧", index = "21"))]
    UnitedKingdom,
    #[strum(serialize = "🇮🇪", serialize = "Ireland", props(flag = "🇮🇪", index = "22"))]
    Ireland,
    #[strum(serialize = "🇦🇺", serialize = "Australia", props(flag = "🇦🇺", index = "23"))]
    Australia,
    #[strum(serialize = "🇳🇿", serialize = "New Zealand", props(flag = "🇳🇿", index = "24"))]
    NewZealand,
}

impl Country {
    fn is_spanish(&self) -> bool {
        lazy_static! {
            static ref UNITED_STATES_INDEX: u64 = Country::UnitedStates
                .get_str("index")
                .expect("Enum didn't have index")
                .parse()
                .expect("Enum index wasn't a number.");
        }

        let index: u64 = self
            .get_str("index")
            .expect("Enum didn't have index")
            .parse()
            .expect("Enum index wasn't a number.");

        index < *UNITED_STATES_INDEX
    }
}

impl Display for Country {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.get_str("flag").expect("Country enum doesn't have flag."))
    }
}

impl From<Country> for NodeIndex {
    fn from(country: Country) -> Self {
        NodeIndex::new(
            country
                .get_str("index")
                .expect("Enum didn't have index.")
                .parse()
                .expect("Enum index wasn't a number."),
        )
    }
}

impl Default for Country {
    fn default() -> Self {
        Country::Argentina
    }
}

#[derive(Debug)]
struct ForvoRecording {
    country: Country,
    recording_link: String,
}

impl ForvoRecording {
    pub fn new(country: Country, recording_link: String) -> ForvoRecording {
        ForvoRecording { country, recording_link }
    }
}

async fn parse_term(ctx: &Context, msg: &Message, args: &mut Args) -> Result<String, NotEnoughArgumentsError> {
    match args.current() {
        Some(arg) => Ok(urlencoding::encode(arg)),
        None => {
            error_util::not_enough_arguments(ctx, &msg.channel_id, 0, 1).await;

            Err(NotEnoughArgumentsError::new(1, 0))
        }
    }
}

fn get_link_and_country(entries: &ElementRef) -> Result<Option<Vec<ForvoRecording>>, Box<dyn StdError + Send + Sync>> {
    lazy_static! {
        static ref FORVO_HTML_MATCHER: Regex = Regex::new(r"(?s)Play\(\d+,'(\w+=*).*?'h'\);return.*? from ([a-zA-Z ]+)").unwrap();
    }

    let mut recordings = Vec::new();

    for capture in FORVO_HTML_MATCHER.captures_iter(entries.inner_html().as_str()) {
        let url_base64_data = capture.get(1).expect("Capture group 1 didn't exist.");
        let country = match capture.get(2).expect("Capture group 2 didn't exist.").as_str().parse::<Country>() {
            Ok(c) => c,
            Err(_) => continue,
        };

        let url_data = String::from_utf8(base64::decode(url_base64_data.as_str())?)?;

        recordings.push(ForvoRecording::new(country, format!("https://forvo.com/mp3/{}", url_data)))
    }

    if recordings.is_empty() {
        Ok(None)
    } else {
        Ok(Some(recordings))
    }
}

async fn get_all_recordings(
    term: &str,
    requested_country: Option<Country>,
) -> Result<[(Option<Vec<ForvoRecording>>, bool); 2], Box<dyn StdError + Send + Sync>> {
    let mut recording_vec = [(None, true), (None, false)];
    let url = format!("https://forvo.com/word/{}/", term);
    let data = FORVO_CLIENT.get(url).send().await?.text().await?;
    let document = Html::parse_document(data.as_str());
    let language_containers = Selector::parse("div.language-container").expect("Bad CSS selector.");
    let (do_english, do_spanish) = match requested_country {
        Some(country) => (!country.is_spanish(), country.is_spanish()),
        None => (true, true),
    };

    for element in document.select(&language_containers) {
        match (element.value().id(), do_spanish, do_english) {
            (Some("language-container-es"), true, _) => recording_vec[0] = (get_link_and_country(&element)?, true),
            (Some("language-container-en"), _, true) => recording_vec[1] = (get_link_and_country(&element)?, false),
            _ => (),
        }
    }

    Ok(recording_vec)
}

async fn get_pronunciation_from_link(forvo_recording: &str) -> Result<Vec<u8>, Error> {
    Ok(FORVO_CLIENT.get(forvo_recording).send().await?.bytes().await?.to_vec())
}

fn get_closest_recording_index(requested_country: Option<Country>, recordings: &[ForvoRecording], is_spanish: bool) -> usize {
    lazy_static! {
        static ref COUNTRY_GRAPH: UnGraph<Country, u32> = UnGraph::from_edges(&[
            (Country::Argentina, Country::Uruguay, 0),
            (Country::Argentina, Country::Chile, 3),
            (Country::Argentina, Country::Peru, 3),
            (Country::Argentina, Country::Paraguay, 2),
            (Country::Chile, Country::Bolivia, 3),
            (Country::Bolivia, Country::Peru, 0),
            (Country::Peru, Country::Paraguay, 3),
            (Country::Bolivia, Country::Ecuador, 1),
            (Country::Ecuador, Country::Colombia, 4),
            (Country::Colombia, Country::Venezuela, 0),
            (Country::Venezuela, Country::DominicanRepublic, 1),
            (Country::Venezuela, Country::Cuba, 1),
            (Country::DominicanRepublic, Country::Cuba, 1),
            (Country::Colombia, Country::Panama, 4),
            (Country::Panama, Country::CostaRica, 1),
            (Country::Panama, Country::Mexico, 2),
            (Country::CostaRica, Country::ElSalvador, 1),
            (Country::ElSalvador, Country::Nicaragua, 1),
            (Country::Nicaragua, Country::Guatemala, 1),
            (Country::Guatemala, Country::Honduras, 1),
            (Country::Honduras, Country::Mexico, 1),
            (Country::Spain, Country::Argentina, 30),
            (Country::UnitedStates, Country::Canada, 1),
            (Country::UnitedStates, Country::Australia, 11),
            (Country::Canada, Country::UnitedKingdom, 10),
            (Country::UnitedKingdom, Country::Australia, 5),
            (Country::UnitedKingdom, Country::Ireland, 4),
            (Country::Australia, Country::NewZealand, 2)
        ]);
        static ref ACCENT_DIFFERENCES: Mutex<HashMap<Country, Vec<Country>>> = Mutex::new(HashMap::new());
        static ref COUNTRY_ENUMS: Vec<Country> = Country::iter().collect();
    }

    // Exit early.
    if recordings.len() == 1 {
        return 0;
    }

    let country = match requested_country {
        Some(country) => country,
        None => {
            if is_spanish {
                Country::Argentina
            } else {
                Country::UnitedStates
            }
        }
    };

    let mut accent_difference_lock = ACCENT_DIFFERENCES.lock().unwrap();
    let distance_vector = accent_difference_lock.entry(country).or_insert_with(|| {
        let distance_map = algo::dijkstra(&*COUNTRY_GRAPH, country.into(), None, |e| *e.weight());
        let mut distances: Vec<(&NodeIndex, &u32)> = distance_map.iter().collect();

        distances.sort_by(|a, b| a.1.cmp(b.1));

        distances.iter().map(|distance| COUNTRY_ENUMS[distance.0.index()]).collect()
    });

    let mut closest_recording_index = usize::MAX;
    let mut closest_country_pos = distance_vector.len();

    for (recording_index, recording) in recordings.iter().enumerate() {
        for (country_index, country) in distance_vector.iter().enumerate() {
            if recording.country == *country && closest_country_pos > country_index {
                closest_country_pos = country_index;
                closest_recording_index = recording_index;
            }
        }
    }

    if closest_recording_index == usize::MAX {
        return 0; // Means that you have recordings from other places.
    }

    closest_recording_index
}

#[derive(Debug)]
pub struct ForvoRecordingData {
    pub recording: Arc<Vec<u8>>,
    pub message: String,
}

pub async fn fetch_pronunciation(
    ctx: &Context,
    msg: &Message,
    args: &mut Args,
) -> Result<Vec<Option<ForvoRecordingData>>, Box<dyn StdError + Send + Sync>> {
    let term = parse_term(ctx, msg, args).await?;

    args.advance();

    let requested_country = if args.remaining() >= 1 {
        Some(commands::parse_choices(ctx, msg, ArgumentInfo::new(args, 1, 2), Country::iter()).await?)
    } else {
        None
    };

    let recordings_data = get_all_recordings(term.as_str(), requested_country).await?;
    let mut links = Vec::with_capacity(2);
    let mut recordings: Vec<_> = recordings_data.iter().map(|recording_option| {
        match recording_option {
            (Some(recordings), is_spanish) => {
                let closest_recording_index = get_closest_recording_index(requested_country, recordings, *is_spanish);
                let recording = &recordings[closest_recording_index];
                let no_special_message = requested_country.map_or(true, |c| c.is_spanish() != *is_spanish);

                let message = if requested_country == Some(recording.country) || no_special_message {
                    format!("Here is the pronunciation of ``{}``. Country: {}.", term, recording.country)
                } else {
                    format!(
                        "Here is the pronunciation of ``{}``. The pronunciation from the country closest in terms of accent to the requested country is {}.",
                        term, recording.country
                    )
                };

                links.push(recording.recording_link.as_str());

                Some(ForvoRecordingData {recording: Arc::new(Vec::new()), message })
            },
            _ => None,
        }
    }).collect();

    for (index, recording_data) in recordings.iter_mut().flatten().enumerate() {
        recording_data.recording = Arc::new(get_pronunciation_from_link(links[index]).await?);
    }

    Ok(recordings)
}
