use super::*;

static STRING_TABLE: std::sync::LazyLock<Mutex<BTreeSet<&'static str>>> =
    std::sync::LazyLock::new(|| Mutex::new(BTreeSet::new()));

static STRING_TRANSFORM_TABLE: std::sync::LazyLock<Mutex<HashMap<(usize, usize), &'static str>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// Intern a string into a process-global table, returning a `&'static str` for it.
pub trait ToStaticStr {
    /// Return a `&'static str` for this value, interning and leaking the bytes the first time a given string is seen.
    fn to_static_str(&self) -> &'static str;
}

impl<T: AsRef<str>> ToStaticStr for T {
    fn to_static_str(&self) -> &'static str {
        let s = self.as_ref();
        let mut string_table = STRING_TABLE.lock();
        if let Some(v) = string_table.get(s) {
            return v;
        }
        let ss = Box::leak(s.to_owned().into_boxed_str());
        string_table.insert(ss);
        ss
    }
}

/// Cache the result of transforming a `&'static str`, keyed by its pointer and length.
pub trait StaticStrTransform {
    /// Return the cached transform of this string, computing it with `transform` on the first call for a given static slice.
    fn static_transform<F: FnOnce(&'static str) -> &'static str>(
        self,
        transform: F,
    ) -> &'static str;
}

impl StaticStrTransform for &'static str {
    fn static_transform<F: FnOnce(&'static str) -> &'static str>(
        self,
        transform: F,
    ) -> &'static str {
        // multiple keys can point to the same data, but it must be bounded due to static lifetime
        // a pointer to static memory plus its length must always be the same immutable slice
        // this is maybe slightly faster for use in log string transformation where speed is essential at scale
        // otherwise we would have used a hash here.
        // TODO: if performance does not suffer, consider switching to a hash at a later point, as this could cause
        // the STRING_TRANSFORM_TABLE to be bigger than necessary, depending on unknowns in rustc about 'static str deduplication.

        let key = (self.as_ptr() as usize, self.len());

        let mut transform_table = STRING_TRANSFORM_TABLE.lock();
        if let Some(v) = transform_table.get(&key) {
            return v;
        }
        let out = transform(self);
        transform_table.insert(key, out);
        out
    }
}
