use std::mem;
use std::ops::{Index, IndexMut};
use std::time::{Duration, SystemTime, SystemTimeError};

use log::warn;
use once_cell::sync::OnceCell;

/// The default load factor to use for the buckets
/// in the search engine.
const DEFAULT_LOAD_FACTOR: usize = 20;

/// The multiplier to apply to the load factor to determine
/// the initial capacity of a bucket. This was arrived at
/// computationally to minimize space wastage.
const INITIAL_CAPACITY_FACTOR: f64 = 1.2;

/// If the number of IDs in the search engine is expected to go below
/// this fraction of the load factor, the bucket array will shrink.
const LOAD_FACTOR_SHRINK_LIMIT: f64 = 3. / 8.;

/// The size of the timestamp within the Discord ID.
const TIMESTAMP_SIZE: u32 = 42;

/// The lowest number of digits possible in a Discord ID.
const MIN_ID_DIGITS: u32 = 17;

/// The first second of Jan 1, 2015.
static DISCORD_EPOCH: OnceCell<SystemTime> = OnceCell::new();

type Id = u64;
type Bucket = Vec<Id>;

const CHOPPED_LOWER_BIT_LIMIT: u32 = Id::BITS - TIMESTAMP_SIZE;

/// THe minimum ID number.
const MIN_ID_NUMBER: Id = (10 as Id).pow(MIN_ID_DIGITS.saturating_sub(1));

struct SnowflakeFuzzyMatch {
    id: Id,
    left_wildcards: u32,
    right_wildcards: u32,
}

impl SnowflakeFuzzyMatch {
    pub fn new(id: Id, left_wildcards: u32, right_wildcards: u32) -> Self {
        Self {
            id,
            left_wildcards,
            right_wildcards,
        }
    }
}

impl PartialEq<Id> for SnowflakeFuzzyMatch {
    fn eq(&self, other: &Id) -> bool {
        fn snowflake_len(mut id: Id) -> u32 {
            // We know all IDs must be at least this many digits
            let mut result = MIN_ID_DIGITS;
            id /= MIN_ID_NUMBER;

            while id >= 10 {
                result += 1;
                id /= 10;
            }

            result
        }

        let mut other = *other;
        let added_digits = self.left_wildcards + self.right_wildcards;

        if added_digits == 0 {
            return self.id == other;
        }

        let total_fuzzy_match_len = snowflake_len(self.id) + added_digits;

        // We can skip the equals if the ID's fuzzy base 10 length isn't equal to
        // other's length.
        // TODO: See if this can be made more efficient or if this check makes it less efficient.
        if total_fuzzy_match_len != snowflake_len(other) {
            return false;
        }

        // Cuts off the left wildcard digits from the original ID
        other %= (10 as Id).pow(total_fuzzy_match_len - self.left_wildcards);

        // Cuts off the right wildcard digits from the original ID
        other /= (10 as Id).pow(self.right_wildcards);

        self.id == other
    }
}

impl PartialEq<SnowflakeFuzzyMatch> for Id {
    fn eq(&self, other: &SnowflakeFuzzyMatch) -> bool {
        other == self
    }
}

/// A memory-efficient search engine that can fuzzy match Discord snowflake IDs.
/// This search engine only can match IDs where any number of digits was chopped off of either
/// or both ends of the ID or anyhwere in between up to the generic const associated with the search engine.
///
/// For example, if the generic const is 2, which is the default, and the ID is ``75385905209671``,
/// then the possible matches are ``3675385905209671XX, 3675385905209671X, X3675385905209671, XX3675385905209671,
/// X3675385905209671X, XX75385905209671X, X75385905209671XX, XX75385905209671XX``.
#[derive(Eq, Debug, Clone)]
pub struct SnowflakeIdSearchEngine<const MAX_DIGITS_CHOPPED: u32 = 2> {
    buckets: Vec<Bucket>,
    len: usize,
    load_factor: usize,
    wildcards: Vec<(u32, u32)>,
}

impl<const T: u32> SnowflakeIdSearchEngine<T> {
    /// The maximum number of bits that will be chopped off from either end of an ID.
    const MAX_BITS_CHOPPED_OFF: u32 = if T == 0 {
        0
    } else {
        // I'm going to hell for this, but taking log2 of an integer is unstable for the moment.
        // This is the same as log2(digits_chopped) + 1.
        u32::BITS - T.leading_zeros()
    };

    /// A number that contains 1's in the bits not occupied by the timestamp of the
    /// snowflake ID.
    const NON_TIMESTAMP_ONES: Id = !(Id::MAX << (Id::BITS - TIMESTAMP_SIZE));

    // TODO: Make a const array of this size when generic_const_exprs stabilizes that contains the
    // the wildcards (u32, u32) and just iterate through that instead in the fuzzy match functions. This is the
    // size of what the array needs to be to hold the elements.
    const WILDCARD_ARRAY_SIZE: usize = (T + 1).pow(2) as usize;

    fn assert_chopped_lower_bit_limit() {
        assert!(
            Self::MAX_BITS_CHOPPED_OFF <= CHOPPED_LOWER_BIT_LIMIT,
            "The amount of bits chopped off by taking away {T} digits from an ID was over the limit of {CHOPPED_LOWER_BIT_LIMIT}."
        );
    }

    /// Returns the max current ID number based on the current system's time.
    /// If it fails due to an incorrect system time, the function will return None.
    /// Because there's a delay between when this function is called and when the
    /// result is used, the max ID number will be stale as soon as the number
    /// is found. Realistically, this shouldn't matter as long as the result is
    /// used within a reasonable amount of time so that no valid user ID
    /// can be generated above this maximum before it's used.
    fn get_max_id_number() -> Option<Id> {
        fn error_on_fail(option: Option<Id>, error: Option<SystemTimeError>) -> Option<Id> {
            if let Some(_) = option {
                return option;
            }

            let msg = "Couldn't get current time relative to Discord epoch due to bad system time. \
            This won't cause incorrect behavior in terms of the expected output, but contains() will act just \
            like no_length_check_contains() and all fuzzy match/contains functions might be less efficient.";

            if let Some(error) = error {
                warn!("{msg} Additional errors caused by this: {error}");
            } else {
                warn!("{msg}");
            }

            return None;
        }

        // The current time constrained to TIMESTAMP_SIZE bits, returning None if the system time is set
        // to a time prior to the Discord epoch or the time in ms since then is more than 42 bits.
        let discord_epoch = DISCORD_EPOCH.get_or_init(|| SystemTime::UNIX_EPOCH + Duration::from_secs(1420070400));
        let current_time = match discord_epoch.elapsed() {
            Ok(time) => error_on_fail(time.as_millis().try_into().ok().filter(|&t| (t >> TIMESTAMP_SIZE) == 0), None)?,
            Err(err) => error_on_fail(None, Some(err))?,
        };

        let shifted_timestamp = current_time << (Id::BITS - TIMESTAMP_SIZE);
        let one_extended_timestamp = shifted_timestamp | Self::NON_TIMESTAMP_ONES;

        Some(one_extended_timestamp)
    }

    fn initialize_wildcard_vector() -> Vec<(u32, u32)> {
        let mut wildcards = Vec::with_capacity(Self::WILDCARD_ARRAY_SIZE);

        for digits_added in 0..=(T * 2) {
            for left_wildcards in digits_added.saturating_sub(T)..=digits_added.min(T) {
                wildcards.push((left_wildcards, digits_added - left_wildcards));
            }
        }

        wildcards
    }

    pub fn new<const MAX_DIGITS_CHOPPED: u32>() -> SnowflakeIdSearchEngine<MAX_DIGITS_CHOPPED> {
        Self::assert_chopped_lower_bit_limit();

        SnowflakeIdSearchEngine::<MAX_DIGITS_CHOPPED> {
            buckets: Vec::new(),
            len: 0,
            load_factor: DEFAULT_LOAD_FACTOR,
            wildcards: Self::initialize_wildcard_vector(),
        }
    }

    pub fn with_load_factor<const MAX_DIGITS_CHOPPED: u32>(load_factor: usize) -> SnowflakeIdSearchEngine<MAX_DIGITS_CHOPPED> {
        Self::assert_chopped_lower_bit_limit();

        SnowflakeIdSearchEngine::<MAX_DIGITS_CHOPPED> {
            buckets: Vec::new(),
            len: 0,
            load_factor,
            wildcards: Self::initialize_wildcard_vector(),
        }
    }

    fn create_buckets(capacity: usize, load_factor: usize) -> Vec<Bucket> {
        let min_bucket_count = capacity / load_factor;
        let min_bucket_count = min_bucket_count.next_power_of_two();

        // We must ensure that the digits we're chopping from the upper bits doesn't cut into the bits we
        // use to determine the bucket index. Since the bucket index is gotten from the lower portion of the timestamp and
        // the timestamp gets cut by MAX_BITS_CHOPPED_OFF bits, TIMESTAMP_SIZE - MAX_BITS_CHOPPED_OFF gets you the number of
        // bits available to use. So the bits used by the bucket index must be less than or equal to this.
        assert!(min_bucket_count.trailing_zeros().max(1) <= TIMESTAMP_SIZE - Self::MAX_BITS_CHOPPED_OFF);

        let mut buckets = Vec::with_capacity(min_bucket_count);
        let bucket_capacity = (load_factor as f64 * INITIAL_CAPACITY_FACTOR) as usize;

        buckets.resize_with(min_bucket_count, || Bucket::with_capacity(bucket_capacity));

        buckets
    }

    pub fn with_capacity<const MAX_DIGITS_CHOPPED: u32>(capacity: usize) -> SnowflakeIdSearchEngine<MAX_DIGITS_CHOPPED> {
        Self::assert_chopped_lower_bit_limit();

        SnowflakeIdSearchEngine::<MAX_DIGITS_CHOPPED> {
            buckets: Self::create_buckets(capacity, DEFAULT_LOAD_FACTOR),
            len: 0,
            load_factor: DEFAULT_LOAD_FACTOR,
            wildcards: Self::initialize_wildcard_vector(),
        }
    }

    pub fn with_capacity_and_load_factor<const MAX_DIGITS_CHOPPED: u32>(
        capacity: usize,
        load_factor: usize,
    ) -> SnowflakeIdSearchEngine<MAX_DIGITS_CHOPPED> {
        Self::assert_chopped_lower_bit_limit();

        SnowflakeIdSearchEngine::<MAX_DIGITS_CHOPPED> {
            buckets: Self::create_buckets(capacity, load_factor),
            len: 0,
            load_factor,
            wildcards: Self::initialize_wildcard_vector(),
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    /// Index is based off of the lower <log2(number of buckets)> bits of the upper [`TIMESTAMP_SIZE`] bits of the ID which is the
    fn get_id_index(bucket_len: usize, id: Id) -> usize {
        debug_assert!(
            bucket_len.is_power_of_two(),
            "The bucket array length should always be a power of two. Got {}",
            bucket_len
        );

        // We want the number of bits the bucket index takes and get just those bits,
        // which is the maximum between the number of trailing zeroes when the number
        // of buckets is a power of two and 1.
        let index_bit_count = bucket_len.trailing_zeros().max(1);
        let index = (id << (TIMESTAMP_SIZE - index_bit_count)) >> (usize::BITS - index_bit_count);

        index as usize
    }

    fn get_bucket(&self, id: Id) -> &[Id] {
        let bucket_index = Self::get_id_index(self.buckets.len(), id);

        self.buckets.index(bucket_index)
    }

    fn get_bucket_mut(&mut self, id: Id) -> &mut Vec<Id> {
        let bucket_index = Self::get_id_index(self.buckets.len(), id);

        self.buckets.index_mut(bucket_index)
    }

    /// Sorts all the buckets in the search engine.
    fn sort_all_buckets(&mut self) {
        for bucket in self.buckets.iter_mut() {
            bucket.sort_unstable();
        }
    }

    fn reallocate_buckets<const SHOULD_SORT: bool>(&mut self, new_capacity: usize) {
        let new_buckets = Self::create_buckets(new_capacity, self.load_factor);
        let old_buckets = mem::replace(&mut self.buckets, new_buckets);
        let new_bucket_len = self.buckets.len();

        // Copy our old bucket vector into our new one that we've swapped into self.buckets.
        for id in old_buckets.into_iter().flatten() {
            let bucket = self.buckets.index_mut(Self::get_id_index(new_bucket_len, id));

            bucket.push(id);
        }

        if SHOULD_SORT {
            self.sort_all_buckets()
        }

        debug_assert!(
            self.buckets.len().is_power_of_two(),
            "The reallocated bucket vector wasn't a power of two.\
             Got length of {}",
            self.buckets.len()
        );
    }

    fn reallocate_on_remove(&mut self, elements_to_be_removed: usize) {
        debug_assert!(
            self.len != 0,
            "The number of IDs in the search engine when calling reallocate_on_remove should never be 0."
        );

        let new_capacity = self.len - elements_to_be_removed;
        let expected_load_factor = new_capacity / self.buckets.len();

        if (expected_load_factor as f64) < self.load_factor as f64 * LOAD_FACTOR_SHRINK_LIMIT {
            self.reallocate_buckets::<true>(new_capacity);
        }
    }

    /// Potentially reallocates the buckets if the load factor is expected be exceeded.
    /// Whether the rebalanced buckets should be sorted or not after this function returns
    /// can be defined as a const generic parameter called SHOULD_SORT. See the note in
    ///  [`add_id_unsorted`] for info on what happens if the buckets are used in an unsorted
    /// state.
    fn reallocate_on_add<const SHOULD_SORT: bool>(&mut self, elements_to_be_added: usize) {
        let new_capacity = self.len + elements_to_be_added;

        // Need a length check to avoid dividing by zero.
        if self.len == 0 || new_capacity / self.buckets.len() > self.load_factor {
            self.reallocate_buckets::<SHOULD_SORT>(new_capacity);
        }
    }

    /// Adds an ID to the search engine. This will expand the capacity of the internal data structures if enough elements are added.
    /// If the ID is already in the search engine, this function might still expand the internal capacity, but won't duplicate the ID
    /// in the search engine. Returns true if the ID was inserted successfully and false if it was already in the search engine.
    /// Panics if the ID's base 10 length is less than 17 as this is not possible for a Discord ID.
    pub fn add_id(&mut self, id: Id) -> bool {
        assert!(id >= MIN_ID_NUMBER, "ID is not of the minimum length, {MIN_ID_DIGITS}.");

        self.reallocate_on_add::<true>(1);

        let bucket = self.get_bucket_mut(id);

        // The binary_search function's Err variant returns where to insert the ID in the bucket.
        match bucket.binary_search(&id) {
            Err(insertion_index) => bucket.insert(insertion_index, id),
            Ok(_) => return false, // An Ok() means it was already in the search engine.
        };

        self.len += 1;

        true
    }

    /// Removes an ID from the search engine. This can shrink the capacity of the internal data structures if enough elements are removed.
    /// Returns true if the ID was found and removed and false if it wasn't in the search engine.
    pub fn remove_id(&mut self, id: Id) -> bool {
        if self.len == 0 {
            return false;
        }

        self.reallocate_on_remove(1);

        let bucket = self.get_bucket_mut(id);

        match bucket.binary_search(&id) {
            Ok(removal_index) => bucket.remove(removal_index),
            Err(_) => return false,
        };

        self.len -= 1;

        true
    }

    pub fn contains(&self, id: Id) -> bool {
        if id < MIN_ID_NUMBER || Self::get_max_id_number().filter(|&max| id > max).is_some() {
            return false;
        }

        self.no_len_check_contains(id)
    }

    pub fn no_len_check_contains(&self, id: Id) -> bool {
        self.get_bucket(id).binary_search(&id).is_ok()
    }

    pub fn fuzzy_contains(&self, id: Id) -> bool {
        self.find_fuzzy_match(id).is_some()
    }

    pub fn find_fuzzy_match(&self, id: Id) -> Option<Id> {
        let max_id = Self::get_max_id_number();

        if max_id.filter(|&max| id > max).is_some() {
            return None;
        }

        let bucket = self.get_bucket(id);

        // Match the exact ID first and do a fuzzy match if it doesn't work.
        bucket.binary_search(&id).ok().map(|_| id).or_else(|| {
            for (left_wildcards, right_wildcards) in self.wildcards.iter().copied() {
                let fuzzy_match = SnowflakeFuzzyMatch::new(id, left_wildcards, right_wildcards);

                // TODO: Make more efficient by breaking early when none of the IDs can be potential matches anymore
                // take advantage of the fact this iterator goes in ascending order.
                // We can also break early while iterating through the buckets, take advantage of that.
                // certain iterations might actually be able to be combined together too

                if let Some(&fuzzy_match) = bucket.iter().filter(|&&id| id == fuzzy_match).next() {
                    return Some(fuzzy_match);
                }
            }

            // TODO: Benchmark if parallelizing the search here would make it more efficient.

            None
        })
    }

    pub fn find_fuzzy_matches(&self, id: Id) -> Vec<Id> {
        let max_id = Self::get_max_id_number();

        if max_id.filter(|&max| id > max).is_some() {
            return Vec::new();
        }

        let bucket = self.get_bucket(id);

        // Match the exact ID first and do a fuzzy match if it doesn't work.
        bucket.binary_search(&id).ok().map(|_| vec![id]).unwrap_or_else(|| {
            let mut fuzzy_matches = Vec::new();

            for (left_wildcards, right_wildcards) in self.wildcards.iter().copied() {
                let fuzzy_match = SnowflakeFuzzyMatch::new(id, left_wildcards, right_wildcards);

                // TODO: Make more efficient by breaking early when none of the IDs can be potential matches anymore
                // take advantage of the fact this iterator goes in ascending order.
                // We can also break early while iterating through the buckets, take advantage of that.
                // certain iterations might actually be able to be combined together too

                fuzzy_matches.extend(bucket.iter().copied().filter(|&id| id == fuzzy_match));
            }

            // TODO: Benchmark if parallelizing the search here would make it more efficient.

            fuzzy_matches
        })
    }
}

impl<const MAX_DIGITS_CHOPPED: u32> Extend<Id> for SnowflakeIdSearchEngine<MAX_DIGITS_CHOPPED> {
    /// Adds the provided [`IntoIterator`] containing [`Id`]s to the search engine.
    /// Any duplicates encountered in the iterator will be ignored.
    /// Panics if any of the IDs in this iterator are below the minimum length of a Discord
    /// snowflake ID, 17.
    fn extend<T: IntoIterator<Item = Id>>(&mut self, iter: T) {
        // if iterator size hint indicates bucket count must be increased to stay in line with load factor, do so
        // size hint can lie so take that into account by enumerating the elements
        // make sure that if this path is taken that we also get rid of duplicates using dedup after sorting
        let iter = iter.into_iter();
        if let (_, Some(upper_bound)) = iter.size_hint() {
            // If we have more than 1/2 the load factor of elements per bucket about to be added, add them unsorted first and sort later, removing duplicates.
            if upper_bound > self.load_factor / 2 * self.buckets.len() {
                self.reallocate_on_add::<false>(upper_bound);

                for (index, id) in iter.enumerate() {
                    assert!(id >= MIN_ID_NUMBER, "ID is not of the minimum length, {MIN_ID_DIGITS}.");

                    if index >= upper_bound {
                        // Size hints can lie, so this check is written just in case.
                        self.reallocate_on_add::<false>(1);
                    }

                    let index = Self::get_id_index(self.buckets.len(), id);

                    self.buckets[index].push(id);
                    self.len += 1;
                }

                self.sort_all_buckets();
                self.buckets.dedup();
            } else {
                for id in iter {
                    self.add_id(id);
                }
            }
        }
    }
}

impl Default for SnowflakeIdSearchEngine {
    fn default() -> SnowflakeIdSearchEngine {
        SnowflakeIdSearchEngine::<2>::new()
    }
}

impl<const N: usize> From<[Id; N]> for SnowflakeIdSearchEngine {
    fn from(array: [Id; N]) -> Self {
        let mut new_search_engine = SnowflakeIdSearchEngine::<2>::with_capacity(N);

        new_search_engine.extend(array);

        new_search_engine
    }
}

impl FromIterator<Id> for SnowflakeIdSearchEngine {
    fn from_iter<T: IntoIterator<Item = Id>>(iter: T) -> Self {
        let iterator = iter.into_iter();
        let upper_bound = iterator.size_hint().1;
        let mut new_search_engine = match upper_bound {
            Some(bound) => SnowflakeIdSearchEngine::<2>::with_capacity(bound),
            None => Default::default(),
        };

        new_search_engine.extend(iterator);

        new_search_engine
    }
}

impl<const MAX_DIGITS_CHOPPED: u32> PartialEq for SnowflakeIdSearchEngine<MAX_DIGITS_CHOPPED> {
    fn eq(&self, other: &Self) -> bool {
        if self.len != other.len {
            return false;
        }

        self.buckets.iter().flatten().copied().all(|id| other.contains(id))
    }
}
