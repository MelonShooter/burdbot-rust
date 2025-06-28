use std::collections::HashMap;
use std::fmt::Debug;
use std::ops::DerefMut;
use std::sync::Mutex;

use base64::DecodeError;
use base64::Engine;
use base64::engine::general_purpose;
use lazy_static::lazy_static;
use log::debug;
use log::error;
use petgraph::algo;
use petgraph::graph::DefaultIx;
use petgraph::graph::NodeIndex;
use petgraph::graph::UnGraph;
use regex::Captures;
use regex::Regex;
use scraper::ElementRef;
use scraper::Html;
use scraper::Selector;
use std::fmt::Display;
use std::string::FromUtf8Error;
use strum::EnumProperty;
use strum::IntoEnumIterator;
use strum::ParseError;
use strum_macros::EnumIter;
use strum_macros::EnumProperty;
use strum_macros::EnumString;
use thiserror::Error;

use crate::util::anti_scraper_download_file;
use crate::util::anti_scraper_get_html;

#[derive(Debug, Copy, Clone)]
pub enum ForvoCaptureType {
    Base64,
    Country,
}

#[derive(Error, Debug, Copy, Clone)]
#[error(
    "Couldn't match capture group {capture_group_idx} for {capture_type:?} from regex string: [ {regex_str} ]."
)]
pub struct ForvoRegexCaptureError {
    pub regex_str: &'static str,
    pub capture_group_idx: usize,
    pub capture_type: ForvoCaptureType,
}

impl ForvoRegexCaptureError {
    pub fn new(
        regex_str: &'static str, capture_group_idx: usize, capture_type: ForvoCaptureType,
    ) -> Self {
        Self { regex_str, capture_group_idx, capture_type }
    }
}

#[non_exhaustive]
#[derive(Error, Debug)]
pub enum ForvoError {
    #[error("Error encountered while fetching forvo recordings. InvalidBase64: {0}")]
    InvalidBase64(#[from] DecodeError),
    #[error("Error encountered while fetching forvo recordings. InvalidUtf8Decode: {0}")]
    InvalidUtf8Decode(#[from] FromUtf8Error),
    #[error("Error encountered while fetching forvo recordings. BadBase64RegexMatching: {0}")]
    BadBase64RegexMatching(#[source] ForvoRegexCaptureError),
    #[error("Error encountered while fetching forvo recordings. BadCountryRegexMatching: {0}")]
    BadCountryRegexMatching(#[source] ForvoRegexCaptureError),
    #[error("Error encountered while fetching forvo recordings. InvalidMatchedCountry: {0}")]
    InvalidMatchedCountry(#[from] ParseError),
    #[error("Error encountered while fetching forvo recordings. IoError: {0}")]
    IoError(#[from] std::io::Error),
}

impl From<ForvoRegexCaptureError> for ForvoError {
    fn from(regex_capture_error: ForvoRegexCaptureError) -> Self {
        match regex_capture_error.capture_type {
            ForvoCaptureType::Base64 => Self::BadBase64RegexMatching(regex_capture_error),
            ForvoCaptureType::Country => Self::BadCountryRegexMatching(regex_capture_error),
        }
    }
}

lazy_static! {
    static ref COUNTRY_GRAPH: UnGraph<Country, u32> = UnGraph::from_edges([
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
        (Country::Australia, Country::NewZealand, 2),
        (Country::Argentina, Country::UnitedStates, u32::MAX / 2) // To take into account edge case. See comment in recording_to_distance.
    ]);
    static ref ACCENT_DIFFERENCES: Mutex<HashMap<(Country, Country), u32>> = Mutex::new(HashMap::new());
    static ref COUNTRY_ENUMS: Vec<Country> = Country::iter().collect();
}

pub type Result<T> = std::result::Result<T, ForvoError>;
type PossibleForvoRecording = Result<ForvoRecording>;

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
enum Language {
    English,
    Spanish,
    Unknown,
}

#[derive(Debug, Default, Hash, PartialEq, Eq, Clone, Copy, EnumIter, EnumString, EnumProperty)]
pub enum Country {
    #[strum(
        serialize = "🇦🇷",
        serialize = "Argentina",
        props(flag = "🇦🇷", index = 0, language = "s")
    )]
    #[default]
    Argentina,
    #[strum(serialize = "🇺🇾", serialize = "Uruguay", props(flag = "🇺🇾", index = 1, language = "s"))]
    Uruguay,
    #[strum(serialize = "🇨🇱", serialize = "Chile", props(flag = "🇨🇱", index = 2, language = "s"))]
    Chile,
    #[strum(serialize = "🇵🇪", serialize = "Peru", props(flag = "🇵🇪", index = 3, language = "s"))]
    Peru,
    #[strum(serialize = "🇧🇴", serialize = "Bolivia", props(flag = "🇧🇴", index = 4, language = "s"))]
    Bolivia,
    #[strum(
        serialize = "🇵🇾",
        serialize = "Paraguay",
        props(flag = "🇵🇾", index = 5, language = "s")
    )]
    Paraguay,
    #[strum(serialize = "🇪🇨", serialize = "Ecuador", props(flag = "🇪🇨", index = 6, language = "s"))]
    Ecuador,
    #[strum(
        serialize = "🇨🇴",
        serialize = "Colombia",
        props(flag = "🇨🇴", index = 7, language = "s")
    )]
    Colombia,
    #[strum(
        serialize = "🇻🇪",
        serialize = "Venezuela",
        props(flag = "🇻🇪", index = 8, language = "s")
    )]
    Venezuela,
    #[strum(serialize = "🇵🇦", serialize = "Panama", props(flag = "🇵🇦", index = 9, language = "s"))]
    Panama,
    #[strum(
        serialize = "🇨🇷",
        serialize = "Costa Rica",
        props(flag = "🇨🇷", index = 10, language = "s")
    )]
    CostaRica,
    #[strum(
        serialize = "🇸🇻",
        serialize = "El Salvador",
        props(flag = "🇸🇻", index = 11, language = "s")
    )]
    ElSalvador,
    #[strum(
        serialize = "🇳🇮",
        serialize = "Nicaragua",
        props(flag = "🇳🇮", index = 12, language = "s")
    )]
    Nicaragua,
    #[strum(
        serialize = "🇬🇹",
        serialize = "Guatemala",
        props(flag = "🇬🇹", index = 13, language = "s")
    )]
    Guatemala,
    #[strum(
        serialize = "🇭🇳",
        serialize = "Honduras",
        props(flag = "🇭🇳", index = 14, language = "s")
    )]
    Honduras,
    #[strum(serialize = "🇲🇽", serialize = "Mexico", props(flag = "🇲🇽", index = 15, language = "s"))]
    Mexico,
    #[strum(serialize = "🇨🇺", serialize = "Cuba", props(flag = "🇨🇺", index = 16, language = "s"))]
    Cuba,
    #[strum(
        serialize = "🇩🇴",
        serialize = "Dominican Republic",
        props(flag = "🇩🇴", index = 17, language = "s")
    )]
    DominicanRepublic,
    #[strum(serialize = "🇪🇸", serialize = "Spain", props(flag = "🇪🇸", index = 18, language = "s"))]
    Spain,
    #[strum(
        serialize = "🇺🇸",
        serialize = "United States",
        props(flag = "🇺🇸", index = 19, language = "e")
    )]
    UnitedStates,
    #[strum(serialize = "🇨🇦", serialize = "Canada", props(flag = "🇨🇦", index = 20, language = "e"))]
    Canada,
    #[strum(
        serialize = "🇬🇧",
        serialize = "United Kingdom",
        props(flag = "🇬🇧", index = 21, language = "e")
    )]
    UnitedKingdom,
    #[strum(
        serialize = "🇮🇪",
        serialize = "Ireland",
        props(flag = "🇮🇪", index = 22, language = "e")
    )]
    Ireland,
    #[strum(
        serialize = "🇦🇺",
        serialize = "Australia",
        props(flag = "🇦🇺", index = 23, language = "e")
    )]
    Australia,
    #[strum(
        serialize = "🇳🇿",
        serialize = "New Zealand",
        props(flag = "🇳🇿", index = 24, language = "e")
    )]
    NewZealand,
}

impl Country {
    fn get_language(self) -> Language {
        match self.get_str("language") {
            Some("s") => Language::Spanish,
            Some("e") => Language::English,
            _ => {
                error!(
                    "Error encountered in the forvo module, get_language(): {self} has an invalid or inexistent language property value."
                );

                Language::Unknown
            },
        }
    }
}

impl Display for Country {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.get_str("flag") {
            Some(flag) => write!(f, "{flag}"),
            None => {
                error!(
                    "Error encountered in the forvo module, Display::fmt(): Couldn't find flag for country idx {:?}.",
                    self.get_int("index")
                );

                write!(f, "UNDEFINED FLAG")
            },
        }
    }
}

impl From<Country> for NodeIndex {
    fn from(country: Country) -> Self {
        let index = match country.get_int("index") {
            Some(num) => num,
            None => {
                error!(
                    "Error encountered in the forvo module, Display::fmt(): Couldn't convert the Country {country} to a node index: \
                     The country's 'index' property doesn't exist. Setting index to the max default node index..."
                );

                return NodeIndex::<DefaultIx>::end();
            },
        };

        NodeIndex::new(index as usize)
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ForvoRecording {
    country: Country,
    recording_link: String,
    language: Language,
}

impl ForvoRecording {
    pub fn new(country: Country, recording_link: String, language: Language) -> ForvoRecording {
        ForvoRecording { country, recording_link, language }
    }
}

fn get_language_recording(
    captures: Captures, regex: &'static str, language: Language,
) -> PossibleForvoRecording {
    let url_base64_data = captures.get(1).ok_or_else(|| {
        ForvoError::BadBase64RegexMatching(ForvoRegexCaptureError::new(
            regex,
            1,
            ForvoCaptureType::Base64,
        ))
    })?;
    let country = captures
        .get(2)
        .ok_or_else(|| {
            ForvoError::BadCountryRegexMatching(ForvoRegexCaptureError::new(
                regex,
                2,
                ForvoCaptureType::Country,
            ))
        })?
        .as_str();

    let country = country.parse::<Country>()?;

    let decoded_bytes = general_purpose::STANDARD.decode(url_base64_data.as_str())?;
    let decoded_link = String::from_utf8(decoded_bytes)?;

    // DO NOT ADD A SLASH AT THE END. THE LINK HAS TO BE WITHOUT ONE
    Ok(ForvoRecording::new(country, format!("https://forvo.com/mp3/{decoded_link}"), language))
}

// TODO: sanitize get_language_recordings to provide a link that has to be from forvo
// TODO: sanitize the download itself to ensure that it is an audio file

/// Gets language recordings for a given language
/// The ElementRef provided should be contain all the play
/// buttons. This function will match all the play buttons and feed them into
/// [`get_language_recording`]
fn get_language_recordings(
    entries: &ElementRef, language: Language,
) -> Vec<PossibleForvoRecording> {
    lazy_static! {
        // The (?s) means to enable the s flag which allows '.' to match newlines
        static ref FORVO_HTML_MATCHER: Regex =
            Regex::new(r"(?s)Play\(\d+,'(\w+=*).*?'h'.*?\);return.*? [fF]rom ([a-zA-Z ]+)").unwrap();
    }

    FORVO_HTML_MATCHER
        .captures_iter(entries.inner_html().as_str())
        .inspect(|c| debug!("Got captures {:.100}..", c.get(0).unwrap().as_str()))
        .map(|captures| get_language_recording(captures, FORVO_HTML_MATCHER.as_str(), language))
        .collect()
}

/// Possible for outer vec to be empty, techinically not possible for inner vec to be empty, but take it into account anyways
async fn get_all_recordings(
    term: &str, requested_country: Option<Country>,
) -> Result<Vec<Vec<PossibleForvoRecording>>> {
    lazy_static! {
        static ref LANGUAGE_CONTAINER_SELECTOR: Selector =
            Selector::parse("div.language-container").expect("Bad CSS selector.");
    }

    const FORVO_HOST: &str = "forvo.com";

    let url = format!("https://{FORVO_HOST}/word/{term}/");
    let data = anti_scraper_get_html(url).await?;
    let document = Html::parse_document(data.as_str());
    let (do_english, do_spanish) = match requested_country.map(|c| c.get_language()) {
        Some(Language::English) => (true, false),
        Some(Language::Spanish) => (false, true),
        _ => (true, true),
    };

    // LANGUAGE_CONTAINER_SELECTOR selects the language container (a div element
    // containing the pronunciations for a particular language)
    // Then this div is further filtered based on its ID, b/c these divs have IDs based
    // on their language, like `language-container-es` for Spanish
    Ok(document
        .select(&LANGUAGE_CONTAINER_SELECTOR)
        .inspect(|e| debug!("Found elem with tag {:?}; {e:?}", e.value().id()))
        .filter_map(|e| match (e.value().id(), do_spanish, do_english) {
            (Some("language-container-es"), true, _) => {
                Some(get_language_recordings(&e, Language::Spanish))
            },
            (Some("language-container-en"), _, true) => {
                Some(get_language_recordings(&e, Language::English))
            },
            _ => None,
        })
        .collect())
}

async fn get_pronunciation_from_link(forvo_recording: &str) -> Result<Vec<u8>> {
    Ok(anti_scraper_download_file(forvo_recording).await?)
}

fn recording_to_distance<T>(
    recording: &ForvoRecording, input_country: Option<Country>, accent_differences: &mut T,
) -> u32
where
    T: DerefMut<Target = HashMap<(Country, Country), u32>>,
{
    let country = match (input_country, recording.language) {
        (Some(country), _) => country,
        (None, Language::English) => Country::UnitedStates,
        (None, Language::Spanish) => Country::Argentina,
        (None, Language::Unknown) => {
            error!(
                "Unknown recording language encountered in forvo module recording_to_distance() while setting a fallback for the input country: {recording:?}.\
                        This should never happen. Returning u32::MAX as the distance..."
            );

            return u32::MAX;
        },
    };

    match accent_differences.get(&(country, recording.country)) {
        Some(&distance) => distance,
        None => {
            let distance_map =
                algo::dijkstra(&*COUNTRY_GRAPH, country.into(), None, |e| *e.weight());
            let mut recording_distance: Option<u32> = None;

            for (node_idx, distance) in distance_map {
                let target_country = COUNTRY_ENUMS[node_idx.index()];

                accent_differences.insert((country, target_country), distance);

                if target_country == recording.country {
                    recording_distance = Some(distance);
                }
            }

            // We should already be taking into account the only case that could've caused None which is when a native of the other set of
            // countries make a non-native recording by setting a graph edge between the 2 to a very high number. This could be caused by
            // distance_map being empty if the starting country given into dijkstra couldn't be validly converted into an index.
            recording_distance.unwrap_or_else(|| {
                error!(
                    "Error encountered in the forvo module recording_to_distance() while calculating recording_distance. \
                     The recording_distance variable was None, which should never happen. This could happen if dijkstra is \
                     returning an empty HashMap as a result of a bad country index in which case an error should've been given \
                     previously for it. Returning u32::MAX as the distance..."
                );

                u32::MAX
            })
        },
    }
}

#[derive(Debug, Clone)]
pub struct RecordingData<'a> {
    pub country: Country,
    pub term: &'a str,
    pub recording_link: String,
    recording: Option<Vec<u8>>,
}

impl<'a> RecordingData<'a> {
    fn new(recording: ForvoRecording, term: &'a str) -> Self {
        Self {
            country: recording.country,
            term,
            recording_link: recording.recording_link,
            recording: None,
        }
    }

    pub async fn get_recording(&mut self) -> Result<(&[u8], Country, &str)> {
        let recording = self
            .recording
            .get_or_insert(get_pronunciation_from_link(self.recording_link.as_str()).await?);

        Ok((recording, self.country, self.term))
    }
}

fn is_closer<T>(
    new: &ForvoRecording, curr: &ForvoRecording, target_country: Option<Country>,
    accent_map: &mut T,
) -> bool
where
    T: DerefMut<Target = HashMap<(Country, Country), u32>>,
{
    // Exit early if new country is same as current, because it will never be closer
    // Same thing if the target country has already been satisfied
    if curr.country == new.country || target_country == Some(curr.country) {
        return false;
    }

    recording_to_distance(new, target_country, accent_map)
        < recording_to_distance(curr, target_country, accent_map)
}

fn possible_recordings_to_data(
    term: &str, country: Option<Country>, possible_recordings: Vec<PossibleForvoRecording>,
) -> impl Iterator<Item = Result<RecordingData<'_>>> {
    let mut possible_data = Vec::new();
    let mut closest_recording = None;
    let mut accent_differences = match ACCENT_DIFFERENCES.lock() {
        Ok(accent_differences) => accent_differences,
        Err(err) => {
            error!(
                "Error encountered in the forvo module, possible_recordings_to_data(): The accent differences hash map lock was poisoned. \
                 This should never happen, but we'll proceed by retrieving the mutex's data anyways."
            );

            err.into_inner()
        },
    };

    for possible_recording in possible_recordings {
        debug!("Comparing curr closest: {closest_recording:?} with curr {possible_recording:?}");

        match (possible_recording, &mut closest_recording) {
            (Ok(curr), Some(min)) => {
                if is_closer(&curr, min, country, &mut accent_differences) {
                    closest_recording = Some(curr)
                }
            },
            (Ok(curr), None) => closest_recording = Some(curr),
            (Err(err), _) => possible_data.push(Err(err)),
        }
    }

    if let Some(closest) = closest_recording {
        possible_data.push(Ok(RecordingData::new(closest, term)));
    }

    possible_data.into_iter()
}

/// Document so closest recordings for english and spanish depending on circumstances are provided, but so are failed recordings
pub async fn fetch_pronunciation(
    term: &str, requested_country: Option<Country>,
) -> Result<Vec<Result<RecordingData<'_>>>> {
    Ok(get_all_recordings(term, requested_country)
        .await?
        .into_iter()
        .flat_map(|possible_recordings| {
            possible_recordings_to_data(term, requested_country, possible_recordings)
        })
        .collect())
}

#[tokio::test]
async fn test_lynx() {
    let forvo = anti_scraper_get_html("https://forvo.com/word/boludo/")
        .await
        .expect("Expected output from lynx");

    assert!(forvo.contains("language-container"));
    assert!(forvo.contains("language-container-es"));

    let sel = Selector::parse("div.language-container").expect("Bad CSS selector.");
    let document = Html::parse_document(forvo.as_str());
    let elements = document.select(&sel).collect::<Vec<_>>();

    assert!(!elements.is_empty());
    assert!(elements.iter().any(|e| e.value().id() == Some("language-container-es")));

    for e in elements {
        let recordings = get_language_recordings(&e, Language::Spanish);

        assert!(!recordings.is_empty());

        let mut last_rec =
            ForvoRecording::new(Country::Argentina, "".to_string(), Language::Spanish);

        for recording in recordings {
            let rec = match recording {
                Ok(r) => r,
                Err(ForvoError::InvalidMatchedCountry(_)) => continue,
                Err(e) => panic!("Got error {e}"),
            };

            assert!(rec.recording_link.starts_with("https://forvo.com/mp3"));
            assert!(rec.recording_link.ends_with(".mp3"));

            println!("rec: {}", rec.recording_link);

            last_rec = rec;
        }

        assert!(!last_rec.recording_link.is_empty());

        let file = match anti_scraper_download_file(last_rec.recording_link).await {
            Ok(f) => f,
            Err(e) => panic!("failed to download file: {e:?}"),
        };

        assert!(file.len() > 3);
        assert!(&file[0..3] == b"ID3"); // Asserts mp3 file has an ID3 header
    }
}
