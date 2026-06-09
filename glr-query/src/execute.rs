use super::query::{Query, QueryMatch};

pub use super::query::NodeRef;

use core::marker::PhantomData;

pub struct QueryMatches<'a>(PhantomData<&'a ()>);

impl<'a> Iterator for QueryMatches<'a> {
    type Item = QueryMatch;

    fn next(&mut self) -> Option<Self::Item> {
        None
    }
}

pub trait Queryable {
    fn query<'a>(&'a self, query: &'a Query) -> QueryMatches<'a>;
}

impl Queryable for glr_core::Tree {
    fn query<'a>(&'a self, _query: &'a Query) -> QueryMatches<'a> {
        QueryMatches(PhantomData)
    }
}
