use core::mem;
use core::ops::{Index, IndexMut};

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

type Id = u64;
type Bucket = Vec<Id>;

const CHOPPED_LOWER_BIT_LIMIT: u32 = Id::BITS - TIMESTAMP_SIZE;

/// THe minimum ID number.
const MIN_ID_NUMBER: Id = (10 as Id).pow(MIN_ID_DIGITS.saturating_sub(1));

// TODO: maybe make this associated fn of SnowflakeFuzzyMatch and add const
// generic to optimize the order reduction.
const fn snowflake_len(mut id: Id) -> u32 {
    const DIGIT_REDUCTION_FROM_MIN: u32 = 4;
    const ORDERS_LESS_MIN: Id = MIN_ID_NUMBER / (10 as Id).pow(DIGIT_REDUCTION_FROM_MIN.saturating_sub(1));

    let mut result = 0;

    if id >= ORDERS_LESS_MIN {
        result += MIN_ID_DIGITS.saturating_sub(DIGIT_REDUCTION_FROM_MIN);
        id /= ORDERS_LESS_MIN;
    }

    while id > 0 {
        result += 1;
        id /= 10;
    }

    result
}

#[derive(Debug, Copy, Clone)]
pub struct FuzzyMatchedId {
    leading_zeros: u8,
    no_leading_zeros_id: Id,
}

impl TryFrom<&str> for FuzzyMatchedId {
    type Error = (); // Since this is used internally, we don't actually care how it errored, just that it did.

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        const MAX_ID_LEN: usize = snowflake_len(Id::MAX) as usize;

        if value.len() > MAX_ID_LEN {
            return Err(());
        }

        if let Some(nonzero_idx) = value.find(|c| c != '0') {
            (&value[nonzero_idx..])
                .parse::<Id>()
                .map(|id| FuzzyMatchedId { leading_zeros: nonzero_idx as u8, no_leading_zeros_id: id })
                .map_err(|_| ())
        } else {
            Ok(FuzzyMatchedId { leading_zeros: (value.len() - 1) as u8, no_leading_zeros_id: 0 })
        }
    }
}

impl TryFrom<Id> for FuzzyMatchedId {
    type Error = (); // TODO: Possibly change to ! when it stabilizes.

    fn try_from(value: Id) -> Result<Self, Self::Error> {
        Ok(FuzzyMatchedId { leading_zeros: 0, no_leading_zeros_id: value })
    }
}

#[derive(Copy, Clone, Debug)]
struct SnowflakeFuzzyMatch {
    fuzzy_id: FuzzyMatchedId,
    left_wildcards: u32,
    right_wildcards: u32,
}

impl SnowflakeFuzzyMatch {
    pub fn new(id: FuzzyMatchedId, left_wildcards: u32, right_wildcards: u32) -> Self {
        Self { fuzzy_id: id, left_wildcards, right_wildcards }
    }
}

impl PartialEq<Id> for SnowflakeFuzzyMatch {
    /// The equality check here has unspecified behavior if other is 0 or 1 because it's simply not possible in our data structure.
    /// It's also not possible for an equality check to be done on a number with more digits (including leading zeros) than the amount
    /// of digits of the highest theoretical ID possible, so this is unspecified behavior too.
    fn eq(&self, other: &Id) -> bool {
        let mut other = *other;
        let added_digits = self.left_wildcards + self.right_wildcards;
        let FuzzyMatchedId { leading_zeros, no_leading_zeros_id } = self.fuzzy_id;

        if added_digits == 0 {
            return no_leading_zeros_id == other;
        }

        let total_fuzzy_match_len = snowflake_len(no_leading_zeros_id).max(1) + added_digits;

        // Check if the numbers we're matching are of the same length
        //  println!("{} {}", total_fuzzy_match_len + leading_zeros as u32, snowflake_len(other));
        if total_fuzzy_match_len + leading_zeros as u32 != snowflake_len(other) {
            return false;
        }

        // Cuts off the left wildcard digits from the original ID
        other %= (10 as Id).saturating_pow(total_fuzzy_match_len + leading_zeros as u32 - self.left_wildcards);

        // Cuts off the right wildcard digits from the original ID
        other /= (10 as Id).pow(self.right_wildcards);

        no_leading_zeros_id == other
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

    fn initialize_wildcard_vector() -> Vec<(u32, u32)> {
        let mut wildcards = Vec::with_capacity(Self::WILDCARD_ARRAY_SIZE);

        for digits_added in 1..=(T * 2) {
            for left_wildcards in digits_added.saturating_sub(T)..=digits_added.min(T) {
                wildcards.push((left_wildcards, digits_added - left_wildcards));
            }
        }

        wildcards
    }

    pub fn new() -> SnowflakeIdSearchEngine<T> {
        Self::assert_chopped_lower_bit_limit();

        SnowflakeIdSearchEngine::<T> { buckets: Vec::new(), len: 0, load_factor: DEFAULT_LOAD_FACTOR, wildcards: Self::initialize_wildcard_vector() }
    }

    pub fn with_load_factor(load_factor: usize) -> SnowflakeIdSearchEngine<T> {
        Self::assert_chopped_lower_bit_limit();

        SnowflakeIdSearchEngine::<T> { buckets: Vec::new(), len: 0, load_factor, wildcards: Self::initialize_wildcard_vector() }
    }

    fn create_buckets(capacity: usize, load_factor: usize) -> Vec<Bucket> {
        // Taken from core's impl of div_ceil because it's not stable
        // TODO: Use std's div_ceil when it's stable.
        pub const fn div_ceil(lhs: usize, rhs: usize) -> usize {
            let d = lhs / rhs;
            let r = lhs % rhs;
            if r > 0 && rhs > 0 {
                d + 1
            } else {
                d
            }
        }

        let min_bucket_count = div_ceil(capacity, load_factor);

        // We need to start out with at least 2 buckets to prevent a shift-right overflow issue in get_id_index().
        let min_bucket_count = min_bucket_count.next_power_of_two().max(2);

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

    pub fn with_capacity(capacity: usize) -> SnowflakeIdSearchEngine<T> {
        Self::assert_chopped_lower_bit_limit();

        SnowflakeIdSearchEngine::<T> {
            buckets: Self::create_buckets(capacity, DEFAULT_LOAD_FACTOR),
            len: 0,
            load_factor: DEFAULT_LOAD_FACTOR,
            wildcards: Self::initialize_wildcard_vector(),
        }
    }

    pub fn with_capacity_and_load_factor(capacity: usize, load_factor: usize) -> SnowflakeIdSearchEngine<T> {
        Self::assert_chopped_lower_bit_limit();

        SnowflakeIdSearchEngine::<T> {
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
        debug_assert!(bucket_len.is_power_of_two(), "The bucket array length should always be a power of two. Got {}", bucket_len);

        // We want the number of bits the bucket index takes and get just those bits,
        // which is the maximum between the number of trailing zeroes when the number
        // of buckets is a power of two.
        let index_bit_count = bucket_len.trailing_zeros();
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
        debug_assert!(self.len != 0, "The number of IDs in the search engine when calling reallocate_on_remove should never be 0.");

        let new_capacity = self.len - elements_to_be_removed;

        if (new_capacity as f64) < (self.load_factor * self.buckets.len()) as f64 * LOAD_FACTOR_SHRINK_LIMIT {
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

        if new_capacity > self.load_factor * self.buckets.len() {
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
        if id < MIN_ID_NUMBER {
            return false;
        }

        self.no_len_check_contains(id)
    }

    pub fn no_len_check_contains(&self, id: Id) -> bool {
        self.get_bucket(id).binary_search(&id).is_ok()
    }

    pub fn fuzzy_contains<S: TryInto<FuzzyMatchedId>>(&self, id: S) -> bool {
        self.find_fuzzy_match(id).is_some()
    }

    pub fn find_fuzzy_match<S: TryInto<FuzzyMatchedId>>(&self, fuzzy_id: S) -> Option<Id> {
        let fuzzy_id = fuzzy_id.try_into().ok()?;
        let id = fuzzy_id.no_leading_zeros_id;
        let bucket = self.get_bucket(id);

        // Match the exact ID first and do a fuzzy match if it doesn't work.
        bucket.binary_search(&id).ok().map(|_| id).or_else(|| {
            for (left_wildcards, right_wildcards) in self.wildcards.iter().copied() {
                let fuzzy_match = SnowflakeFuzzyMatch::new(fuzzy_id, left_wildcards, right_wildcards);

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

    pub fn find_fuzzy_matches<S: TryInto<FuzzyMatchedId>>(&self, fuzzy_id: S) -> Vec<Id> {
        let fuzzy_id = match fuzzy_id.try_into() {
            Ok(id) => id,
            Err(_) => return Vec::new(),
        };

        let id = fuzzy_id.no_leading_zeros_id;
        let bucket = self.get_bucket(id);

        // Match the exact ID first and do a fuzzy match if it doesn't work.
        bucket.binary_search(&id).ok().map(|_| vec![id]).unwrap_or_else(|| {
            let mut fuzzy_matches = Vec::new();

            for (left_wildcards, right_wildcards) in self.wildcards.iter().copied() {
                let fuzzy_match = SnowflakeFuzzyMatch::new(fuzzy_id, left_wildcards, right_wildcards);

                // TODO: Make more efficient by breaking early when none of the IDs can be potential matches anymore
                // take advantage of the fact this iterator goes in ascending order.
                // We can also break early while iterating through the buckets, take advantage of that.
                // certain iterations might actually be able to be combined together too

                fuzzy_matches.extend(bucket.iter().filter(|&&id| id == fuzzy_match));
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

#[cfg(test)]
mod test {

    use std::collections::HashSet;

    // TODO:
    // Test length and other internal state after adding, extending, and removing
    // Test all the contains and fuzzy matching functions to ensure they return the correct thing
    // Test error cases in assert_chopped_lower_bit_limit, create_buckets, all ctors, add_id, and extend (using #[should_panic] attribute)
    // write tests in a dedicated test folder combining creating search engines in all 4 initial states, making sure they're empty, getting elements
    // inserting, removing elements, checking contains, and fuzzy matching
    // DOCUMENT
    // then do a practical test on all existing users in span-eng server, fuzzy matching
    // non-existent and existent IDs too all of them
    // write benchmarks
    use lazy_static::lazy_static;
    use rand::distributions::Uniform;
    use rand::{Rng, SeedableRng};

    use crate::id_search_engine::*;

    use super::{FuzzyMatchedId, Id, SnowflakeFuzzyMatch, MIN_ID_NUMBER};

    const REALISTIC_MAX_ID: Id = 999_999_999_999_999_999; // This is a possible 18 digit timestamp for 2022-07-22T11:22:59.101Z.

    #[test]
    fn fuzzy_matched_id_creation_test() {
        let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(129388342034342);

        for _ in 0..10000 {
            let id = rng.gen::<Id>();
            let fuzzy_id = FuzzyMatchedId::try_from(id).unwrap();

            assert_eq!(fuzzy_id.leading_zeros, 0);
            assert_eq!(fuzzy_id.no_leading_zeros_id, id);
        }

        for _ in 0..10000 {
            let rand_id = rng.gen::<Id>() / 1000;
            let mut id = rand_id.to_string();
            let num_leading_zeros = rng.gen_range(0..3);

            for _ in 0..num_leading_zeros {
                id.insert(0, '0');
            }

            let fuzzy_id = FuzzyMatchedId::try_from(id.as_str()).unwrap();

            assert_eq!(fuzzy_id.leading_zeros, num_leading_zeros);
            assert_eq!(fuzzy_id.no_leading_zeros_id, rand_id);
        }
    }

    fn random_realistic_snowflakes() -> &'static [Id] {
        lazy_static! {
            static ref RANDOM_SNOWFLAKES: Vec<Id> = {
                let rng = rand_pcg::Pcg64Mcg::seed_from_u64(129388342034342);

                rng.sample_iter(Uniform::new_inclusive(MIN_ID_NUMBER, REALISTIC_MAX_ID)).take(1_000_000).collect::<Vec<_>>()
            };
        }

        &*RANDOM_SNOWFLAKES
    }

    #[test]
    fn snowflake_len_test() {
        assert_eq!(snowflake_len(861128391953352906), 18);
        assert_eq!(snowflake_len(83919533), 8);

        let mut rand = rand_pcg::Pcg64Mcg::seed_from_u64(123863);

        for len in 6..20 {
            for _ in 0..100_000 {
                // Test with randomized float [0.1, 1) multiplied by 10^(desired length) casted to integers.
                // We use floats to ensure an even distribution across orders.
                let random_float: f64 = rand.gen_range(0.1..1.0);
                let random_id = random_float * 10u64.pow(len) as f64;

                assert_eq!(
                    len,
                    snowflake_len(random_id as Id),
                    "Snowflake len test failed. Length of snowflake: {len}. \
                     Got length: {}. The snowflake was {}",
                    snowflake_len(random_id as Id),
                    random_id as Id
                );
            }
        }
    }

    #[test]
    fn snowflake_fuzzy_match_test() {
        for id in
            rand_pcg::Pcg64Mcg::seed_from_u64(432563546374).sample_iter(Uniform::new_inclusive(10_000_000_000, MIN_ID_NUMBER / 100)).take(10_000)
        {
            let mut fuzzy_1 = SnowflakeFuzzyMatch { fuzzy_id: id.try_into().unwrap(), left_wildcards: 2, right_wildcards: 2 };
            let mut id_string = id.to_string();
            id_string.insert_str(0, "72");
            id_string.push_str("19");

            let id = id_string.parse().unwrap();

            assert_eq!(fuzzy_1, id);

            id_string.pop();
            let id = id_string.parse().unwrap();

            assert_ne!(fuzzy_1, id);

            fuzzy_1.right_wildcards -= 1;

            assert_eq!(fuzzy_1, id);

            fuzzy_1.left_wildcards -= 1;
            fuzzy_1.right_wildcards += 1;

            assert_ne!(fuzzy_1, id);

            fuzzy_1.left_wildcards += 1;
            fuzzy_1.right_wildcards -= 1;

            fuzzy_1.left_wildcards -= 1;
            id_string.remove(0);
            let id = id_string.parse().unwrap();

            assert_eq!(fuzzy_1, id);

            fuzzy_1.left_wildcards -= 1;
            id_string.remove(0);
            let id = id_string.parse().unwrap();

            assert_eq!(fuzzy_1, id);

            fuzzy_1.right_wildcards -= 1;
            id_string.pop();
            fuzzy_1.left_wildcards += 1;
            id_string.insert(0, '2');
            let id = id_string.parse().unwrap();

            assert_eq!(fuzzy_1, id);

            fuzzy_1.left_wildcards -= 1;
            fuzzy_1.right_wildcards += 1;

            assert_ne!(fuzzy_1, id);
        }
    }

    fn gen_fuzzy_match(str: &str, lower: usize, upper: usize) -> SnowflakeFuzzyMatch {
        let id = &str[lower..str.len() - upper];

        SnowflakeFuzzyMatch {
            fuzzy_id: id.try_into().expect("IDs in tests should always be valid numbers."),
            left_wildcards: lower as u32,
            right_wildcards: upper as u32,
        }
    }

    #[test]
    fn realistic_snowflake_fuzzy_match_true_cases_test() {
        let snowflakes = random_realistic_snowflakes();

        // true test case to test out
        for snowflake in snowflakes.iter().copied().take(10_000) {
            let str = snowflake.to_string();

            for i in 0..6 {
                for j in 0..6 {
                    let snowflake_match = gen_fuzzy_match(str.as_str(), i, j);

                    assert_eq!(snowflake_match, snowflake);
                }
            }
        }
    }

    #[test]
    fn realistic_snowflake_fuzzy_match_false_cases_test() {
        fn gen_number_length_not_num(num: Id, len: usize, rand: &mut impl Iterator<Item = char>) -> String {
            let num_as_str = num.to_string();
            let mut number = String::with_capacity(len); // Generate number that's the same length, but not the snowflake

            while number.is_empty() || number == num_as_str {
                number.clear();

                for _ in 0..len {
                    let digit = rand.next().unwrap();

                    number.push(digit);
                }
            }

            number
        }

        let rand = rand_pcg::Pcg64Mcg::seed_from_u64(854342512);
        let mut char_gen = rand.sample_iter(Uniform::new_inclusive('0', '9'));
        let snowflakes = random_realistic_snowflakes();

        for snowflake in snowflakes.iter().copied().take(10_000) {
            let str = snowflake.to_string();

            for left in 0..4 {
                for right in 0..4 {
                    let mut same_len_non_snowflake_1 = gen_number_length_not_num(snowflake, str.len(), &mut char_gen);
                    let subtracted_fuzzy_match = gen_fuzzy_match(same_len_non_snowflake_1.as_str(), left, right);

                    assert_ne!(subtracted_fuzzy_match, snowflake);

                    for _ in 0..left {
                        same_len_non_snowflake_1.insert(0, char_gen.next().unwrap());
                    }

                    for _ in 0..right {
                        same_len_non_snowflake_1.push(char_gen.next().unwrap());
                    }

                    assert_ne!(gen_fuzzy_match(same_len_non_snowflake_1.as_str(), left, right), snowflake);
                }
            }
        }
    }

    #[test]
    fn snowflake_leading_zero_test() {
        fn create_fuzzy_snowflake(left: u32, right: u32, leading_zeros: u8, id: Id) -> SnowflakeFuzzyMatch {
            let fuzzy_id = FuzzyMatchedId { leading_zeros, no_leading_zeros_id: id };

            SnowflakeFuzzyMatch::new(fuzzy_id, left, right)
        }

        // In all of these tests, we don't care whether it matches 0 or 1 or not because our data structure prevents 0 or 1 from being inserted.
        // Tests 0, which shouldn't match 1000
        // Tests 0000
        // Tests 0300, which should match just 300
        // Tests X0 which should match 50
        // Tests X0000 which should match 20000
        // Tests X0300, which should match 10300 and 90300 but not 300 or 2300
        // Tests X0009245, which should match 30009245 but not 5009245 or 709245 or 69245 or 9245
        // Test XX0X which should match 8705 and 1000 but not 10000 or 604 or 3
        // Test XX000000X which should match 740000000 and 100000000, but not 43000000 or 1000000000
        // Test XX005951X which should match 760059513 and 100059510, but not 4460059515 or 3790059515

        let zero = create_fuzzy_snowflake(0, 0, 0, 0);
        let four_zero = create_fuzzy_snowflake(0, 0, 3, 0);
        let zero_300 = create_fuzzy_snowflake(0, 0, 1, 300);
        let x_0 = create_fuzzy_snowflake(1, 0, 0, 0);
        let x_0000 = create_fuzzy_snowflake(1, 0, 3, 0);
        let x_0300 = create_fuzzy_snowflake(1, 0, 1, 300);
        let x_0009245 = create_fuzzy_snowflake(1, 0, 3, 9245);
        let xx_0_x = create_fuzzy_snowflake(2, 1, 0, 0);
        let xx_000000_x = create_fuzzy_snowflake(2, 1, 5, 0);
        let xx_005951_x = create_fuzzy_snowflake(2, 1, 2, 5951);

        assert_ne!(zero, 1000);

        assert_ne!(four_zero, 10);
        assert_ne!(four_zero, 100);
        assert_ne!(four_zero, 1000);
        assert_ne!(four_zero, 10000);

        assert_eq!(zero_300, 300);
        assert_ne!(zero_300, 30);
        assert_ne!(zero_300, 3000);
        assert_ne!(zero_300, 30000);

        assert_eq!(x_0, 50);
        assert_ne!(x_0, 500);

        assert_eq!(x_0000, 20000);
        assert_eq!(x_0000, 80000);
        assert_ne!(x_0000, 2000);
        assert_ne!(x_0000, 200);
        assert_ne!(x_0000, 20);
        assert_ne!(x_0000, 2);

        assert_eq!(x_0300, 10300);
        assert_eq!(x_0300, 90300);
        assert_ne!(x_0300, 300);
        assert_ne!(x_0300, 2300);

        assert_eq!(x_0009245, 3_0009245);
        assert_ne!(x_0009245, 5009245);
        assert_ne!(x_0009245, 709245);
        assert_ne!(x_0009245, 69245);
        assert_ne!(x_0009245, 9245);

        assert_eq!(xx_0_x, 8705);
        assert_eq!(xx_0_x, 1000);
        assert_ne!(xx_0_x, 10000);
        assert_ne!(xx_0_x, 604);
        assert_ne!(xx_0_x, 3);

        assert_eq!(xx_000000_x, 740000005);
        assert_eq!(xx_000000_x, 100000000);
        assert_ne!(xx_000000_x, 43000000);
        assert_ne!(xx_000000_x, 1000000000);

        assert_eq!(xx_005951_x, 760059513);
        assert_eq!(xx_005951_x, 100059510);
        assert_ne!(xx_005951_x, 4460059515);
        assert_ne!(xx_005951_x, 3790059515);
    }

    #[test]
    fn init_wildcard_array_test() {
        let vec = SnowflakeIdSearchEngine::<0>::initialize_wildcard_vector();
        let vec_2 = SnowflakeIdSearchEngine::<1>::initialize_wildcard_vector();
        let vec_3 = SnowflakeIdSearchEngine::<2>::initialize_wildcard_vector();
        let vec_4 = SnowflakeIdSearchEngine::<3>::initialize_wildcard_vector();

        assert_eq!(vec, vec![]);
        assert_eq!(vec_2, vec![(0, 1), (1, 0), (1, 1)]);
        assert_eq!(vec_3, vec![(0, 1), (1, 0), (0, 2), (1, 1), (2, 0), (1, 2), (2, 1), (2, 2)]);
        assert_eq!(
            vec_4,
            vec![(0, 1), (1, 0), (0, 2), (1, 1), (2, 0), (0, 3), (1, 2), (2, 1), (3, 0), (1, 3), (2, 2), (3, 1), (2, 3), (3, 2), (3, 3)]
        );
    }

    #[test]
    fn create_buckets_test() {
        let buckets = SnowflakeIdSearchEngine::<2>::create_buckets(67_000, 20);

        assert_eq!(buckets.len(), 4096);

        let buckets_2 = SnowflakeIdSearchEngine::<2>::create_buckets(250_000, 10);

        assert_eq!(buckets_2.len(), 32768);

        for bucket in buckets {
            assert!(bucket.capacity() >= (20f64 * INITIAL_CAPACITY_FACTOR) as usize);
        }

        for bucket in buckets_2 {
            assert!(bucket.capacity() >= (10f64 * INITIAL_CAPACITY_FACTOR) as usize);
        }
    }

    #[test]
    fn test_default_ctor() {
        let search_engine = SnowflakeIdSearchEngine::<3>::new();

        assert_eq!(search_engine.buckets.capacity(), 0);
        assert_eq!(search_engine.len, 0);
        assert_eq!(search_engine.load_factor, DEFAULT_LOAD_FACTOR);
        assert_eq!(
            search_engine.wildcards,
            vec![(0, 1), (1, 0), (0, 2), (1, 1), (2, 0), (0, 3), (1, 2), (2, 1), (3, 0), (1, 3), (2, 2), (3, 1), (2, 3), (3, 2), (3, 3)]
        );
    }

    #[test]
    fn test_with_load_ctor() {
        let search_engine = SnowflakeIdSearchEngine::<1>::with_load_factor(50);

        assert_eq!(search_engine.buckets.capacity(), 0);
        assert_eq!(search_engine.len, 0);
        assert_eq!(search_engine.load_factor, 50);
        assert_eq!(search_engine.wildcards, vec![(0, 1), (1, 0), (1, 1)]);
    }

    #[test]
    fn test_with_capacity_ctor() {
        let search_engine = SnowflakeIdSearchEngine::<2>::with_capacity(7831);

        assert!(search_engine.buckets.capacity() >= 512);
        assert_eq!(search_engine.len, 0);
        assert_eq!(search_engine.load_factor, DEFAULT_LOAD_FACTOR);
        assert_eq!(search_engine.wildcards, vec![(0, 1), (1, 0), (0, 2), (1, 1), (2, 0), (1, 2), (2, 1), (2, 2)]);
    }

    #[test]
    fn test_with_load_and_capacity_ctor() {
        let search_engine = SnowflakeIdSearchEngine::<2>::with_capacity_and_load_factor(65000, 10);

        assert!(search_engine.buckets.capacity() >= 8192);
        assert_eq!(search_engine.len, 0);
        assert_eq!(search_engine.load_factor, 10);
        assert_eq!(search_engine.wildcards, vec![(0, 1), (1, 0), (0, 2), (1, 1), (2, 0), (1, 2), (2, 1), (2, 2)]);
    }

    #[test]
    fn add_unique_ids_and_expansion_test() {
        let capacity = 256 * DEFAULT_LOAD_FACTOR;
        let mut search_engine = SnowflakeIdSearchEngine::<2>::with_capacity(capacity);
        let num_buckets = search_engine.buckets.len();
        let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(5834024).sample_iter(Uniform::new(MIN_ID_NUMBER, REALISTIC_MAX_ID));
        let unique_ids = rng.by_ref().take(capacity).collect::<HashSet<_>>();

        for id in unique_ids.iter().copied() {
            assert!(search_engine.add_id(id), "Unique ID caused add_id to return false.");
        }

        // Given the default laod factor, the number of buckets never should've changed
        assert_eq!(num_buckets, search_engine.buckets.len(), "The search engine bucket array shouldn't have expanded yet.");

        // Adding one more element should cause the number of buckets to double though
        assert!(search_engine.add_id(rng.filter(|id| (!unique_ids.contains(id))).next().unwrap()), "Unique ID caused add_id to return false.");

        assert_eq!(num_buckets * 2, search_engine.buckets.len(), "The search engine bucket array never expanded.");
        assert_eq!(search_engine.len(), capacity + 1, "The length isn't correct.");
        assert!(search_engine.buckets.len().is_power_of_two(), "Search engine bucket array length not a power of two.");

        for (idx, bucket) in search_engine.buckets.iter().enumerate() {
            assert!(
                bucket.windows(2).all(|e| e[0] < e[1]),
                "Bucket {idx} wasn't sorted or somehow had duplicates even though all IDs inserted were unique. Bucket state: {bucket:?}"
            );

            for id in bucket.iter().copied() {
                let idx_len = format!("{:b}", search_engine.buckets.len()).len() as u64 - 1; // It's a power of two so this gets a potential index's length.

                assert_eq!(
                    idx as u64,
                    (id << (TIMESTAMP_SIZE as u64 - idx_len)) >> (Id::BITS as u64 - idx_len),
                    "{id} was in the wrong bucket. Was in bucket {idx}"
                )
            }
        }
    }

    #[test]
    fn add_duplicate_ids_test() {
        let capacity = 256 * DEFAULT_LOAD_FACTOR;
        let mut search_engine = SnowflakeIdSearchEngine::<2>::with_capacity(capacity);
        let rng = rand_pcg::Pcg64Mcg::seed_from_u64(5834024).sample_iter(Uniform::new(MIN_ID_NUMBER, REALISTIC_MAX_ID));
        let unique_ids = rng.take(capacity).collect::<HashSet<_>>().into_iter().collect::<Vec<_>>();
        let rand_idxs = rand_pcg::Pcg64Mcg::seed_from_u64(634241).sample_iter(Uniform::new(0, unique_ids.len()));
        let duplicates = rand_idxs.take(capacity / 5).map(|idx| unique_ids[idx]);

        for id in unique_ids.iter().copied() {
            assert!(search_engine.add_id(id), "Unique ID caused add_id to return false.");
        }

        // We will add a few elements that are duplicates
        for duplicate in duplicates {
            assert!(!search_engine.add_id(duplicate), "Duplicate ID caused add_id to return true.");
        }

        assert_eq!(search_engine.len(), capacity, "The length isn't correct. Duplicates shouldn't increase the search engine's length");

        for (idx, bucket) in search_engine.buckets.iter().enumerate() {
            assert!(bucket.windows(2).all(|e| e[0] < e[1]), "Bucket {idx} wasn't sorted or had duplicates. Bucket state: {bucket:?}");

            for id in bucket.iter().copied() {
                let idx_len = format!("{:b}", search_engine.buckets.len()).len() as u64 - 1; // It's a power of two so this gets a potential index's length.

                assert_eq!(
                    idx as u64,
                    (id << (TIMESTAMP_SIZE as u64 - idx_len)) >> (Id::BITS as u64 - idx_len),
                    "{id} was in the wrong bucket. Was in bucket {idx}"
                )
            }
        }
    }

    #[test]
    fn eq_test() {
        let mut search_engine = SnowflakeIdSearchEngine::<2>::new();
        let mut search_engine_2 = SnowflakeIdSearchEngine::<2>::with_capacity(78645);
        let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(234);
        let rand_to_insert = rng.gen_range(MIN_ID_NUMBER..REALISTIC_MAX_ID);
        let rand_vec = rng.sample_iter(Uniform::new(MIN_ID_NUMBER, REALISTIC_MAX_ID)).take(4096).collect::<Vec<_>>();

        for rand in rand_vec.iter().copied() {
            if rand_to_insert == rand {
                continue;
            }

            search_engine.add_id(rand);
        }

        assert_eq!(search_engine.clone(), search_engine);

        for rand in rand_vec.iter().copied() {
            if rand_to_insert == rand {
                continue;
            }

            search_engine_2.add_id(rand);
        }

        assert_eq!(search_engine, search_engine_2);

        search_engine_2.add_id(rand_to_insert);

        assert_ne!(search_engine, search_engine_2);
    }

    #[test]
    fn contains_test() {
        let mut search_engine = SnowflakeIdSearchEngine::<2>::new();
        let rng = rand_pcg::Pcg64Mcg::seed_from_u64(242395723);
        let mut id_gen = rng.sample_iter(Uniform::new_inclusive(MIN_ID_NUMBER, REALISTIC_MAX_ID));
        let id_set = id_gen.by_ref().take(100_000).collect::<HashSet<_>>();
        let id_vec = id_gen.take(100_000).filter(|id| !id_set.contains(id)).collect::<Vec<_>>();

        for id in id_vec.iter().copied() {
            search_engine.add_id(id);
        }

        for id in id_vec {
            assert!(search_engine.contains(id), "Search engine doesn't contain value that it should: {id}.");
        }

        for id in id_set {
            assert!(!search_engine.contains(id), "Search engine contains value that it shouldn't: {id}.");
        }
    }

    #[test]
    fn remove_test() {
        // Add random unique elements to the list
        // Remove elements (collect added elements into a hashset and a vec so we can get a rand index, but also make unique)
        // Remove elements not in the hashset
        let bucket_count = 256;
        let capacity = bucket_count * DEFAULT_LOAD_FACTOR;
        let mut search_engine = SnowflakeIdSearchEngine::<2>::with_capacity(capacity);
        let rng = rand_pcg::Pcg64Mcg::seed_from_u64(5834024).sample_iter(Uniform::new(MIN_ID_NUMBER, REALISTIC_MAX_ID));
        let unique_ids_set = rng.take(capacity).collect::<HashSet<_>>();
        let unique_ids = unique_ids_set.iter().copied().collect::<Vec<_>>();

        for id in unique_ids.iter().copied() {
            search_engine.add_id(id);
        }

        // This is to ensure we don't accidentally shrink the search engine.
        let elements_to_take = capacity as f64 * (LOAD_FACTOR_SHRINK_LIMIT * 1.5);

        let random_unique_idxs = rand_pcg::Pcg64Mcg::seed_from_u64(6452312)
            .sample_iter(Uniform::new(0, unique_ids.len()))
            .map(|idx| unique_ids[idx])
            .take(elements_to_take as usize)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        for id in random_unique_idxs.iter().copied() {
            assert!(
                search_engine.remove_id(id),
                "Removal of element in search engine caused remove_id() to return false. ID that caused this: {id}."
            );
        }

        assert_eq!(unique_ids.len() - random_unique_idxs.len(), search_engine.len, "Length of the search engine wasn't correct after removals.");

        for id in random_unique_idxs.iter().copied() {
            assert!(!search_engine.contains(id), "Search engine still contains element that was removed. ID that caused this: {id}.");
        }

        let rand_id_gen = rand_pcg::Pcg64Mcg::seed_from_u64(21831)
            .sample_iter(Uniform::new(MIN_ID_NUMBER, REALISTIC_MAX_ID))
            .take(10_000)
            .filter(|id| !unique_ids_set.contains(id));

        for id in random_unique_idxs.iter().copied().chain(rand_id_gen) {
            assert!(
                !search_engine.remove_id(id),
                "Removal of element not in search engine caused remove_id() to return false. ID that caused this: {id}."
            );
        }

        // The search engine shouldn't have shrunk at this point.
        assert_eq!(search_engine.buckets.len(), bucket_count);
        assert_eq!(
            unique_ids.len() - random_unique_idxs.len(),
            search_engine.len,
            "Length of the search engine shouldn't have changed after non-existent removals."
        );
    }

    #[test]
    fn remove_shrink_test() {
        let bucket_count = 256;
        let capacity = bucket_count * DEFAULT_LOAD_FACTOR;
        let mut search_engine = SnowflakeIdSearchEngine::<2>::with_capacity(capacity);
        let rng = rand_pcg::Pcg64Mcg::seed_from_u64(5834024).sample_iter(Uniform::new(MIN_ID_NUMBER, REALISTIC_MAX_ID));
        let unique_ids_set = rng.take(capacity).collect::<HashSet<_>>();
        let unique_ids = unique_ids_set.iter().copied().collect::<Vec<_>>();

        for id in unique_ids.iter().copied() {
            search_engine.add_id(id);
        }

        for id in unique_ids.into_iter().take(((capacity as f64 * (1. - LOAD_FACTOR_SHRINK_LIMIT)) as usize) + 1) {
            search_engine.remove_id(id);
        }

        assert!(
            search_engine.buckets.len() < bucket_count,
            "The search engine never shrunk. The bucket count is {} and the current number of elements \
             in the search engine is {}",
            search_engine.buckets.len(),
            search_engine.len()
        );
    }

    #[test]
    fn extend_test() {}

    #[test]
    fn extend_unsorted_insertion_test() {}
}
