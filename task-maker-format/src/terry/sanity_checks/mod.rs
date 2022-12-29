use crate::sanity_checks::{SanityCheck, SanityChecks};
use crate::terry::TerryTask;

mod checker;
mod statement;
mod task;

inventory::collect!(&'static dyn SanityCheck<TerryTask>);

/// Make a new `SanityChecks` for a IOI task skipping the checks with the provided names.
pub fn get_sanity_checks(skip: &[String]) -> SanityChecks<TerryTask> {
    SanityChecks::new(get_sanity_check_list(skip))
}

/// Return the list of sanity checks excluding the ones with their name in the provided list.
fn get_sanity_check_list(skip: &[String]) -> Vec<&'static dyn SanityCheck<TerryTask>> {
    inventory::iter::<&dyn SanityCheck<TerryTask>>()
        .cloned()
        .filter(|s| !skip.contains(&s.name().into()))
        .collect()
}

/// Return a comma separated list of the names of all the sanity checks.
pub fn get_sanity_check_names() -> Vec<&'static str> {
    get_sanity_check_list(&[])
        .iter()
        .map(|s| s.name())
        .collect()
}
