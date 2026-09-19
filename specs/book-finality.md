---
tags: reference, cip
crystal-type: process
crystal-domain: cyber
status: draft
alias: book finality, domain finality per book
---

# book finality

property 31 of [[cyber/launch|launch]]: domain finality per book — a book settles locally, cross-book conditions wait on evidence. this is [[oikos]] foundation 2 in foculus's own terms: no new mechanism, the existing local/certified split of [[finality|finality]] and the portable [[FinalityEvidence]] object, read at the book granularity.

## a book is a domain

each personal chain ([[oikos]]) is its own [[Domain]] — its own ε-support over its own graph, its own [[tip|tip]] (height, root). `finalizes()` (protocol.md step 6) runs entirely inside one book: the adaptive threshold and the certification gate are both local reads over that book's φ*, never the planetary graph. a book settling locally is exactly a particle reaching `Finality::Final` in its own domain.

## cross-book conditions wait on evidence

a condition that spans two books (oikos "the two settlement protocols") cannot re-run the source book's consensus — it waits for `FinalityEvidence`:

```
book A: finalizes(signal, domain_A, ...) → Final
        FinalityEvidence::issue_certified(signal, tip_A, nullifiers)
book B: holds a pending condition on signal @ book A
        condition resolves iff evidence.verify(tip_A)
```

`verify` rebinds signal_id, height, root and nullifier hash and checks `grade4()`; it fails on a stale, wrong or absent tip, so a condition genuinely waits — it cannot resolve against B's own state, only A's actual one. `issue_from_domain` refuses to emit certified evidence at all while `finalizes()` is `Pending`, so there is nothing for B to accept until A actually settles: this is the "wait" half of the property, not a courtesy check.

the partition rule oikos foundation 2 asks for ("a partition retains pending obligations and expires conditions according to their rules") is a caller-side policy over this primitive — a condition with no evidence stays pending indefinitely unless the caller attaches its own expiry, which is not part of this contract.

## what this does not bind yet

`bind()` commits domain-tag, signal_id, height, root, nullifier hash and grade4 — it does not commit a book identifier. two books whose height and 32-byte root happen to collide would produce interchangeable evidence; in practice a book's root is a hemera commitment over its whole state, so an accidental collision is exactly as unlikely as any other particle collision. an explicit book-id in the binding is a hardening step for later, not required to exercise the property for phase 1.

## exercised on two books

`tests/book_finality.rs`: book A settles a particle locally, independent of book B's own unrelated tip; book B's cross-book condition on that particle stays unresolved while A has not certified it, and resolves the moment A's certified evidence verifies against A's actual tip — and not against a stale one.

see [[finality]] for the domain-local test · [[FinalityEvidence]] for the portable certificate · [[oikos]] for the four foundations this is one of.
