use announce_test_derive::fetched_raw;

// A plain macro that is not a trampoline: it forwards its input to the next
// list element and adds no entry. The raw `Input::fetch` fetched it once and
// gets no definition back, so it must stop with the same error as `gather`
// instead of fetching until the recursion limit.
macro_rules! forwards_only {
    (@announce_format "1" ($($o:tt)*)
     [$($h:ident)? $(:: $hs:ident)* ! , $($n:ident)? $(:: $ns:ident)* ! $($tail:tt)*]
     $($r:tt)*) => {
        $($n)? $(:: $ns)* ! { @announce_format "1" ($($o)*) [$($n)? $(:: $ns)* ! $($tail)*] $($r)* }
    };
}

fetched_raw! { [fetched_raw!] (forwards_only) F }

// The same with a fetch that follows a real one.
fetched_raw! { [fetched_raw!] (::announce_test_defs::kinds::Shape, forwards_only) G }

fn main() {}
