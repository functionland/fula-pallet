// SBP-M1 review: comment does not match crate name
//! Benchmarking setup for sugarfunge-fula

use super::*;

#[allow(unused)]
use crate::Pallet as fula;
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_system::RawOrigin;

// SBP-M1 review: missing benchmarks for 18 dispatchable functions
// SBP-M1 review: add ci to require successful benchmark tests before merging
benchmarks! {
    // SBP-M1 review: remove sample benchmark
    do_something {
        let s in 0 .. 100;
        let caller: T::AccountId = whitelisted_caller();
    }: _(RawOrigin::Signed(caller), s)
    verify {
        assert_eq!(Something::<T>::get(), Some(s));
    }

    impl_benchmark_test_suite!(fula, crate::mock::new_test_ext(), crate::mock::Test);
}
