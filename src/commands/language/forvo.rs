mod error;

pub use error::*;
use regex::Captures;

use std::collections::HashMap;
use std::fmt::Debug;
use std::ops::DerefMut;
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

lazy_static! {
    static ref FORVO_CLIENT: Client = Client::new();
}

type ForvoResult<T> = Result<T, ForvoError>;
type PossibleForvoRecording = ForvoResult<ForvoRecording>;

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
enum Language {
    English,
    Spanish,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, EnumIter, EnumString, EnumProperty)]
pub enum Country {
    #[strum(serialize = "🇦🇷", serialize = "Argentina", props(flag = "🇦🇷", index = "0", language = "s"))]
    Argentina,
    #[strum(serialize = "🇺🇾", serialize = "Uruguay", props(flag = "🇺🇾", index = "1", language = "s"))]
    Uruguay,
    #[strum(serialize = "🇨🇱", serialize = "Chile", props(flag = "🇨🇱", index = "2", language = "s"))]
    Chile,
    #[strum(serialize = "🇵🇪", serialize = "Peru", props(flag = "🇵🇪", index = "3", language = "s"))]
    Peru,
    #[strum(serialize = "🇧🇴", serialize = "Bolivia", props(flag = "🇧🇴", index = "4", language = "s"))]
    Bolivia,
    #[strum(serialize = "🇵🇾", serialize = "Paraguay", props(flag = "🇵🇾", index = "5", language = "s"))]
    Paraguay,
    #[strum(serialize = "🇪🇨", serialize = "Ecuador", props(flag = "🇪🇨", index = "6", language = "s"))]
    Ecuador,
    #[strum(serialize = "🇨🇴", serialize = "Colombia", props(flag = "🇨🇴", index = "7", language = "s"))]
    Colombia,
    #[strum(serialize = "🇻🇪", serialize = "Venezuela", props(flag = "🇻🇪", index = "8", language = "s"))]
    Venezuela,
    #[strum(serialize = "🇵🇦", serialize = "Panama", props(flag = "🇵🇦", index = "9", language = "s"))]
    Panama,
    #[strum(serialize = "🇨🇷", serialize = "Costa Rica", props(flag = "🇨🇷", index = "10", language = "s"))]
    CostaRica,
    #[strum(serialize = "🇸🇻", serialize = "El Salvador", props(flag = "🇸🇻", index = "11", language = "s"))]
    ElSalvador,
    #[strum(serialize = "🇳🇮", serialize = "Nicaragua", props(flag = "🇳🇮", index = "12", language = "s"))]
    Nicaragua,
    #[strum(serialize = "🇬🇹", serialize = "Guatemala", props(flag = "🇬🇹", index = "13", language = "s"))]
    Guatemala,
    #[strum(serialize = "🇭🇳", serialize = "Honduras", props(flag = "🇭🇳", index = "14", language = "s"))]
    Honduras,
    #[strum(serialize = "🇲🇽", serialize = "Mexico", props(flag = "🇲🇽", index = "15", language = "s"))]
    Mexico,
    #[strum(serialize = "🇨🇺", serialize = "Cuba", props(flag = "🇨🇺", index = "16", language = "s"))]
    Cuba,
    #[strum(serialize = "🇩🇴", serialize = "Dominican Republic", props(flag = "🇩🇴", index = "17", language = "s"))]
    DominicanRepublic,
    #[strum(serialize = "🇪🇸", serialize = "Spain", props(flag = "🇪🇸", index = "18", language = "s"))]
    Spain,
    #[strum(serialize = "🇺🇸", serialize = "United States", props(flag = "🇺🇸", index = "19", language = "e"))]
    UnitedStates,
    #[strum(serialize = "🇨🇦", serialize = "Canada", props(flag = "🇨🇦", index = "20", language = "e"))]
    Canada,
    #[strum(serialize = "🇬🇧", serialize = "United Kingdom", props(flag = "🇬🇧", index = "21", language = "e"))]
    UnitedKingdom,
    #[strum(serialize = "🇮🇪", serialize = "Ireland", props(flag = "🇮🇪", index = "22", language = "e"))]
    Ireland,
    #[strum(serialize = "🇦🇺", serialize = "Australia", props(flag = "🇦🇺", index = "23", language = "e"))]
    Australia,
    #[strum(serialize = "🇳🇿", serialize = "New Zealand", props(flag = "🇳🇿", index = "24", language = "e"))]
    NewZealand,
}

impl Country {
    fn get_language(self) -> Language {
        match self.get_str("language") {
            Some("s") => Language::Spanish,
            Some("e") => Language::English,
            _ => panic!("{self} has an invalid or inexistent language property value."),
        }
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
    language: Language,
}

impl ForvoRecording {
    pub fn new(country: Country, recording_link: String, language: Language) -> ForvoRecording {
        ForvoRecording {
            country,
            recording_link,
            language,
        }
    }
}

fn get_language_recording(captures: Captures, regex: &'static str, language: Language) -> PossibleForvoRecording {
    // TODO: refactor vocaroo error handling to follow same model
    let url_base64_data = captures
        .get(1)
        .ok_or_else(|| ForvoError::BadBase64RegexMatching(ForvoRegexCaptureError::new(regex, 1, ForvoCaptureType::Base64)))?;
    let country = captures
        .get(2)
        .ok_or_else(|| ForvoError::BadCountryRegexMatching(ForvoRegexCaptureError::new(regex, 2, ForvoCaptureType::Country)))?
        .as_str();
    //.trim(); // Necessary because some countries have leading/trailing whitespace for whatever reason.
    let country = country.parse::<Country>()?;

    let decoded_bytes = base64::decode(url_base64_data.as_str())?;
    let decoded_link = String::from_utf8(decoded_bytes)?;

    Ok(ForvoRecording::new(country, format!("https://forvo.com/mp3/{}", decoded_link), language))
}

/// Gets language recordings for a given language
fn get_language_recordings(entries: &ElementRef, language: Language) -> Vec<PossibleForvoRecording> {
    lazy_static! {
        static ref FORVO_HTML_MATCHER: Regex = Regex::new(r"(?s)Play\(\d+,'(\w+=*).*?'h'\);return.*? from ([a-zA-Z ]+)").unwrap();
    }

    FORVO_HTML_MATCHER
        .captures_iter(entries.inner_html().as_str())
        .map(|captures| get_language_recording(captures, FORVO_HTML_MATCHER.as_str(), language))
        .collect()
}

fn to_opposite_tuple(b: bool) -> (bool, bool) {
    (b, !b)
}

/// Possible for outer vec to be empty, techinically not possible for inner vec to be empty, but take it into account anyways
async fn get_all_recordings(term: &str, requested_country: Option<Country>) -> ForvoResult<Vec<Vec<PossibleForvoRecording>>> {
    let url = format!("https://forvo.com/word/{}/", term);
    let data = FORVO_CLIENT.get(url).send().await?.text().await?;
    let document = Html::parse_document(data.as_str());
    let language_containers = Selector::parse("div.language-container").expect("Bad CSS selector.");
    let (do_english, do_spanish) = match requested_country {
        Some(country) => to_opposite_tuple(country.get_language() == Language::English),
        None => (true, true),
    };

    Ok(document
        .select(&language_containers)
        .filter_map(|e| match (e.value().id(), do_spanish, do_english) {
            (Some("language-container-es"), true, _) => Some(get_language_recordings(&e, Language::Spanish)),
            (Some("language-container-en"), _, true) => Some(get_language_recordings(&e, Language::English)),
            _ => None,
        })
        .collect())
}

async fn get_pronunciation_from_link(forvo_recording: &str) -> Result<Vec<u8>, Error> {
    Ok(FORVO_CLIENT.get(forvo_recording).send().await?.bytes().await?.to_vec())
}

fn recording_to_distance<T: DerefMut<Target = HashMap<(Country, Country), u32>>>(
    recording: &ForvoRecording,
    input_country: Option<Country>,
    accent_difference_map: &mut T,
    country_graph: &UnGraph<Country, u32>,
    country_index_lookup: &[Country],
) -> u32 {
    let accent_difference_map = accent_difference_map.deref_mut();
    let country = input_country.unwrap_or_else(|| match recording.language {
        Language::English => Country::UnitedStates,
        Language::Spanish => Country::Argentina,
    });

    let dist = match accent_difference_map.get(&(country, recording.country)) {
        Some(&distance) => distance,
        None => {
            let distance_map = algo::dijkstra(country_graph, country.into(), None, |e| *e.weight());
            let mut recording_distance: Option<u32> = None;

            for (node_idx, distance) in distance_map {
                let target_country = country_index_lookup[node_idx.index()];

                accent_difference_map.insert((country, target_country), distance);

                if target_country == recording.country {
                    recording_distance = Some(distance);
                }
            }

            debug_assert_ne!(recording_distance, None); // Recording distance should always be set within the for loop.

            recording_distance.unwrap()
        }
    };

    dist
}

fn get_closest_recording<'a>(requested_country: Option<Country>, recordings: &[PossibleForvoRecording]) -> Option<&ForvoRecording> {
    lazy_static! {
        static ref COUNTRY_GRAPH: UnGraph<Country, u32> = UnGraph::from_edges(&[
            (Country::Argentina, Country::Uruguay, 1),
            (Country::Argentina, Country::Chile, 3),
            (Country::Argentina, Country::Peru, 3),
            (Country::Argentina, Country::Paraguay, 2),
            (Country::Chile, Country::Bolivia, 3),
            (Country::Bolivia, Country::Peru, 1),
            (Country::Peru, Country::Paraguay, 3),
            (Country::Bolivia, Country::Ecuador, 2),
            (Country::Ecuador, Country::Colombia, 4),
            (Country::Colombia, Country::Venezuela, 1),
            (Country::Venezuela, Country::DominicanRepublic, 2),
            (Country::Venezuela, Country::Cuba, 2),
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
        static ref ACCENT_DIFFERENCES: Mutex<HashMap<(Country, Country), u32>> = Mutex::new(HashMap::new());
        static ref COUNTRY_ENUMS: Vec<Country> = Country::iter().collect();
    }

    let mut map = ACCENT_DIFFERENCES.lock().expect("Lock can't be poisoned here");

    recordings
        .into_iter()
        .filter_map(|r| r.as_ref().ok())
        .min_by_key(|r| recording_to_distance(r, requested_country, &mut map, &*COUNTRY_GRAPH, &*COUNTRY_ENUMS))
}

#[derive(Debug, Clone)]
pub struct RecordingData<'a> {
    pub country: Country,
    pub term: &'a str,
    pub recording_link: String,
    recording: Option<Vec<u8>>,
}

impl<'a> RecordingData<'a> {
    pub fn new(country: Country, term: &'a str, recording_link: String) -> Self {
        Self {
            country,
            term,
            recording_link,
            recording: None,
        }
    }

    pub fn has_recording(&self) -> bool {
        self.recording.is_some()
    }

    pub async fn get_recording(&mut self) -> ForvoResult<&[u8]> {
        if let None = self.recording {
            self.recording = Some(get_pronunciation_from_link(self.recording_link.as_str()).await?);
        }

        Ok(self.recording.as_deref().unwrap())
    }
}

fn possible_recordings_to_data<'a>(
    term: &'a str,
    requested_country: Option<Country>,
    possible_recordings: Vec<PossibleForvoRecording>,
) -> impl Iterator<Item = ForvoResult<RecordingData<'a>>> + 'a {
    let closest_recording = get_closest_recording(requested_country, &possible_recordings);
    let recording_data = closest_recording.map(|r| Ok(RecordingData::new(r.country, term, r.recording_link.clone())));

    // TODO: figure out how to fix this clone

    possible_recordings
        .into_iter()
        .filter_map(|res| match res {
            Ok(_) => None,
            Err(e) => Some(Err(e)),
        })
        .chain(recording_data)
}

/// Document so closest recordings for english and spanish depending on circumstances are provided, but so are failed recordings
pub async fn fetch_pronunciation<'a>(term: &'a str, requested_country: Option<Country>) -> ForvoResult<Vec<ForvoResult<RecordingData<'a>>>> {
    Ok(get_all_recordings(term, requested_country)
        .await?
        .into_iter()
        .flat_map(|possible_recordings| possible_recordings_to_data(term, requested_country, possible_recordings))
        .collect())
}
