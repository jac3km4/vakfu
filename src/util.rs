use std::marker::PhantomData;

use byte::{BytesExt, TryRead};

#[derive(Debug, Clone)]
pub struct WithSizePrefix<C, L> {
    ctx: C,
    len: PhantomData<L>,
}

impl<C, L> WithSizePrefix<C, L> {
    pub fn new(ctx: C) -> Self {
        Self {
            ctx,
            len: PhantomData,
        }
    }
}

impl<'a, A, C, L> TryRead<'a, WithSizePrefix<C, L>> for Vec<A>
where
    A: TryRead<'a, C>,
    C: Copy,
    L: Into<usize> + TryRead<'a, C>,
{
    fn try_read(
        bytes: &'a [u8],
        WithSizePrefix { ctx, .. }: WithSizePrefix<C, L>,
    ) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        let len: L = bytes.read(offset, ctx)?;
        let vec = bytes
            .read_iter(offset, ctx)
            .take(len.into())
            .collect::<byte::Result<Vec<A>>>()?;
        Ok((vec, *offset))
    }
}

pub struct WithSize<C>(C, usize);

impl<C> WithSize<C> {
    pub fn new(ctx: C, count: usize) -> Self {
        Self(ctx, count)
    }
}

impl<'a, A, C> TryRead<'a, WithSize<C>> for Vec<A>
where
    A: TryRead<'a, C>,
    C: Copy,
{
    fn try_read(bytes: &'a [u8], WithSize(ctx, count): WithSize<C>) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        let vec = bytes
            .read_iter(offset, ctx)
            .take(count)
            .collect::<byte::Result<Vec<A>>>()?;
        Ok((vec, *offset))
    }
}
