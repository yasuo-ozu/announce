use announce_test_derive::gathered;

// A plain macro that is not a trampoline: it forwards its input to the next
// list element and adds no entry. `gather` fetched it once and gets no
// definition back, so it must stop with an error instead of fetching forever.
macro_rules! forwards_only {
    (($($o:tt)*)
     [$($h:ident)? $(:: $hs:ident)* ! , $($n:ident)? $(:: $ns:ident)* ! $($tail:tt)*]
     $($r:tt)*) => {
        $($n)? $(:: $ns)* ! { ($($o)*) [$($n)? $(:: $ns)* ! $($tail)*] $($r)* }
    };
}

gathered! { [gathered!] (forwards_only) G }

fn main() {}
