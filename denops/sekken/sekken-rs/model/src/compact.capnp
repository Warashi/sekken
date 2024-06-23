@0xe53731c862edacd2;

struct CompactModel {
  unigram @0 :List(UnigramEntry);
  bigram @1 :List(BigramEntry);
  struct UnigramEntry {
    key @0 :UInt32;
    value @1 :UInt8;
  }
  struct BigramEntry {
    key @0 :UInt64;
    value @1 :UInt8;
  }
}
