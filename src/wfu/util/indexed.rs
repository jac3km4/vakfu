pub trait Indexed<K, V> {
    fn at(&mut self, k: K) -> Option<V>;
}
