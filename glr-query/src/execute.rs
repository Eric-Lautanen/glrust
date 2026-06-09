use super::query::{Query, QueryMatch};

pub use super::query::NodeRef;

pub struct QueryMatches<'a> {
    query: &'a Query,
    tree: &'a glr_core::Tree,
}

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
    fn query<'a>(&'a self, query: &'a Query) -> QueryMatches<'a> {
        QueryMatches { query, tree: self }
    }
}
