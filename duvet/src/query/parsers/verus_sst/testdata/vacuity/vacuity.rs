// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0
use vstd::prelude::*;

verus! {

// ---- The "implementation" under test -------------------------------
// An impl annotation would sit on the body lines (12-14).
pub open spec fn spec_add_one(x: int) -> int {
    x + 1
}

pub fn add_one(x: u32) -> (r: u32)
    requires x < 1000,
    ensures r == spec_add_one(x as int),
{
    let r = x + 1;
    r
}

// ---- Scenario A: honest proof --------------------------------------
// References the implementation's spec. Expected: closure reaches
// spec_add_one / add_one's contract spans.
pub proof fn honest_proof()
    ensures forall|x: int| spec_add_one(x) == x + 1,
{
}

// ---- Scenario B: vacuous proof, never mentions the impl ------------
// requires false; ensures something absurd. Never references add_one.
// Expected (user hypothesis, case 1): tiny closure, no impl spans.
pub proof fn vacuous_proof(x: int)
    requires x != x,
    ensures x == x + 1,
{
}

// ---- Scenario B2: vacuous proof that MENTIONS the impl -------------
// requires false, but the ensures references spec_add_one.
// Discriminates elaboration-record vs needed-record semantics:
// if SST records what elaboration touched, spec_add_one appears here
// even though the solver needed nothing.
pub proof fn vacuous_proof_mentioning(x: int)
    requires x != x,
    ensures spec_add_one(x) == x - 100,
{
}

// ---- Scenario C: the user's scenario --------------------------------
// Impl annotation inside THIS function's body (line with `let y`);
// test annotation on the vacuously-true ensures.
// Question: does the fn-level witness closure include its own body
// (self-inclusion), discharging the pair despite vacuity?
pub fn self_contained(x: u32) -> (r: u32)
    ensures x != x ==> r == 999,
{
    let y = x;
    y
}

// ---- Scenario D: unit labels from the artifact ----------------------
// A loop with a proof_note'd invariant and a proof_note'd assert,
// plus unnoted siblings. Pins Decision 20 label extraction:
// ProofNoteLabel text when recorded, span identity otherwise.
pub fn noted_loop(n: u32) -> (r: u32)
    requires n <= 100,
    ensures r <= 100,
{
    let mut i: u32 = 0;
    while i < n
        invariant
            #[verifier::proof_note("i stays bounded")]
            i <= n,
            n <= 100,
        decreases n - i,
    {
        i = i + 1;
    }
    proof {
        #[verifier::proof_note("loop exit bound")]
        assert(i <= 100);
        assert(i >= 0);
    }
    i
}

} // verus!

fn main() {}
