# Write the spec

Parent: [SPL Meter MVP](../map.md)
Type: task
Status: open
Blocked by: 01, 02, 03, 04, 05, 06, 07, 08, 09, 11

## Question

Nothing left to decide — assemble the resolved tickets into the spec that is this map's
destination, and get Simon's approval on it.

The spec must be complete enough that an implementation session needs no further
decisions: audio backend and permissions, weighting filter approach and coefficients,
the three level metrics and how they're computed, the calibration model and persistence,
spectrogram parameters, the Rust↔frontend contract, and the screen layout.

Publish it at `.scratch/spl-meter-mvp/spec.md` per the local-tracker convention.

State the accuracy limitations plainly — broadband-offset calibration only, phone mic, no
IEC 61672 conformance claim — so the spec doesn't imply an instrument this isn't.
