@0xa4907e3630fe1936;

struct HiddenMarkovModel {
  nHidden @0 :UInt64;
  nObserved @1 :UInt64;

  chars @2 :List(Char);

  initial @3 :List(Float64);
  transition @4 :List(Float64);
  emission @5 :List(Float64);

  struct Char {
    id @0 :UInt64;
    code @1 :UInt32; # unicode code point
  }
}
