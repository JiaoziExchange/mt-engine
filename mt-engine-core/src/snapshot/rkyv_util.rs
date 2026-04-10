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
    T: Archive + Default,
    T::Archived: Deserialize<T, D>,
    D: Fallible + ?Sized,
{
    fn deserialize_with(
        field: &rkyv::vec::ArchivedVec<Option<T::Archived>>,
        deserializer: &mut D,
    ) -> Result<Slab<T>, D::Error> {
        let mut slab = Slab::with_capacity(field.len());
        for (next_idx, item) in field.iter().enumerate() {
            if let Some(archived) = item.as_ref() {
                let val: T = archived.deserialize(deserializer)?;
                let idx = slab.insert(val);
                assert_eq!(idx, next_idx, "Slab index misalignment!");
            } else {
                let idx = slab.insert(T::default());
                slab.remove(idx);
                assert_eq!(
                    idx, next_idx,
                    "Slab index misalignment during hole creation!"
                );
            }
        }
        Ok(slab)
    }
}
