use crate::sanity_checks::{SanityCheck, SanityChecks};
use crate::terry::TerryTask;

mod checker;
mod statement;
mod task;

inventory::collect!(&'static dyn SanityCheck<TerryTask>);

/// Make a new `SanityChecks` for a IOI task skipping the checks with the provided names.
pub fn get_sanity_checks(skip: &[&str]) -> SanityChecks<TerryTask> {
    SanityChecks::new(get_sanity_check_list(skip))
}

/// Return the list of sanity checks excluding the ones with their name in the provided list.
pub fn get_sanity_check_list(skip: &[&str]) -> Vec<&'static dyn SanityCheck<TerryTask>> {
    inventory::iter::<&dyn SanityCheck<TerryTask>>()
        .cloned()
        .filter(|s| !skip.contains(&s.name()) && !skip.contains(&s.category().as_str()))
        .collect()
}
