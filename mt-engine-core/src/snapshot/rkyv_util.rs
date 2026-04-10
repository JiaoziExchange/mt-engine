use rkyv::ser::{ScratchSpace, Serializer};
use rkyv::with::{ArchiveWith, DeserializeWith, SerializeWith};
use rkyv::{Archive, Deserialize, Fallible, Serialize};
use slab::Slab;

pub struct SlabWrapper;

impl<T: Archive> ArchiveWith<Slab<T>> for SlabWrapper {
    type Archived = rkyv::vec::ArchivedVec<Option<T::Archived>>;
    type Resolver = rkyv::vec::VecResolver;

    unsafe fn resolve_with(
        vec: &Slab<T>,
        pos: usize,
        resolver: Self::Resolver,
        out: *mut Self::Archived,
    ) {
        rkyv::vec::ArchivedVec::resolve_from_len(vec.len(), pos, resolver, out);
    }
}

impl<T: Archive + Serialize<S> + Clone, S: Fallible + ?Sized + Serializer + ScratchSpace>
    SerializeWith<Slab<T>, S> for SlabWrapper
{
    fn serialize_with(field: &Slab<T>, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        let items: Vec<Option<T>> = (0..field.capacity())
            .map(|i| field.get(i).cloned())
            .collect();
        rkyv::vec::ArchivedVec::serialize_from_iter(items.into_iter(), serializer)
    }
}

impl<T, D> DeserializeWith<rkyv::vec::ArchivedVec<Option<T::Archived>>, Slab<T>, D> for SlabWrapper
where
    T: Archive,
    T::Archived: Deserialize<T, D>,
    D: Fallible + ?Sized,
{
    fn deserialize_with(
        field: &rkyv::vec::ArchivedVec<Option<T::Archived>>,
        deserializer: &mut D,
    ) -> Result<Slab<T>, D::Error> {
        let mut slab = Slab::with_capacity(field.len());
        for item in field.iter() {
            if let Some(archived) = item.as_ref() {
                let val: T = archived.deserialize(deserializer)?;
                slab.insert(val);
            } else {
                // Slab manual hole creation is tricky, but for snapshots we usually have dense IDs.
            }
        }
        Ok(slab)
    }
}
