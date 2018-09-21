// Copyright 2015-2018 Parity Technologies (UK) Ltd.
// This file is part of Parity.

// Parity is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity.  If not, see <http://www.gnu.org/licenses/>.

//! Generates trie root.
//!
//! This module should be used to generate trie root hash.

extern crate hashdb;
extern crate triestream;
#[cfg(test)]
extern crate keccak_hasher;
#[cfg(test)]
extern crate parity_codec;

use std::collections::BTreeMap;
use std::cmp;
use std::fmt::Debug; // TODO: remove when done here along with all the `Debug` bounds

use hashdb::Hasher;

use triestream::TrieStream;

fn shared_prefix_len<T: Eq>(first: &[T], second: &[T]) -> usize {
	first.iter()
		.zip(second.iter())
		.position(|(f, s)| f != s)
		.unwrap_or_else(|| cmp::min(first.len(), second.len()))
}

/// Generates a trie root hash for a vector of key-value tuples
///
/// ```rust
/// extern crate triehash;
/// extern crate keccak_hasher;
/// extern crate triestream;
/// use triehash::trie_root;
/// use keccak_hasher::KeccakHasher;
/// use triestream::RlpTrieStream;
///
/// fn main() {
/// 	let v = vec![
/// 		("doe", "reindeer"),
/// 		("dog", "puppy"),
/// 		("dogglesworth", "cat"),
/// 	];
///
/// 	let root = "8aad789dff2f538bca5d8ea56e8abe10f4c7ba3a5dea95fea4cd6e7c3a1168d3";
/// 	assert_eq!(trie_root::<KeccakHasher, RlpTrieStream, _, _, _>(v), root.into());
/// }
/// ```
pub fn trie_root<H, S, I, A, B>(input: I) -> H::Out
	where I: IntoIterator<Item = (A, B)>,
		  A: AsRef<[u8]> + Ord + Debug,
		  B: AsRef<[u8]> + Debug,
		  H: Hasher,
		  S: TrieStream,
{

	// first put elements into btree to sort them and to remove duplicates
	let input = input
		.into_iter()
		.collect::<BTreeMap<_, _>>();

	let mut nibbles = Vec::with_capacity(input.keys().map(|k| k.as_ref().len()).sum::<usize>() * 2);
	let mut lens = Vec::with_capacity(input.len() + 1);
	lens.push(0);
	for k in input.keys() {
		for &b in k.as_ref() {
			nibbles.push(b >> 4);
			nibbles.push(b & 0x0F);
		}
		lens.push(nibbles.len());
	}

	// then move them to a vector
	let input = input.into_iter().zip(lens.windows(2))
		.map(|((_, v), w)| (&nibbles[w[0]..w[1]], v))
		.collect::<Vec<_>>();

	let mut stream = S::new();
	build_trie::<H, S, _, _>(&input, 0, &mut stream);
	H::hash(&stream.out())
}

#[cfg(test)]
pub fn unhashed_trie<H, S, I, A, B>(input: I) -> Vec<u8>
	where I: IntoIterator<Item = (A, B)> + Debug,
		  A: AsRef<[u8]> + Ord + Debug,
		  B: AsRef<[u8]> + Debug,
		  H: Hasher,
		  S: TrieStream,
{

	// first put elements into btree to sort them and to remove duplicates
	let input = input
		.into_iter()
		.collect::<BTreeMap<_, _>>();

	let mut nibbles = Vec::with_capacity(input.keys().map(|k| k.as_ref().len()).sum::<usize>() * 2);
	let mut lens = Vec::with_capacity(input.len() + 1);
	lens.push(0);
	for k in input.keys() {
		for &b in k.as_ref() {
			nibbles.push(b >> 4);
			nibbles.push(b & 0x0F);
		}
		lens.push(nibbles.len());
	}

	// then move them to a vector
	let input = input.into_iter().zip(lens.windows(2))
		.map(|((_, v), w)| (&nibbles[w[0]..w[1]], v))
		.collect::<Vec<_>>();

	// println!("as nibbles: {:#x?}", input);
	let mut stream = S::new();
	build_trie::<H, S, _, _>(&input, 0, &mut stream);
	stream.out()
}

/// Generates a key-hashed (secure) trie root hash for a vector of key-value tuples.
///
/// ```rust
/// extern crate triehash;
/// extern crate keccak_hasher;
/// extern crate triestream;
/// use triehash::sec_trie_root;
/// use keccak_hasher::KeccakHasher;
/// use triestream::RlpTrieStream;
///
/// fn main() {
/// 	let v = vec![
/// 		("doe", "reindeer"),
/// 		("dog", "puppy"),
/// 		("dogglesworth", "cat"),
/// 	];
///
/// 	let root = "d4cd937e4a4368d7931a9cf51686b7e10abb3dce38a39000fd7902a092b64585";
/// 	assert_eq!(sec_trie_root::<KeccakHasher, RlpTrieStream, _, _, _>(v), root.into());
/// }
/// ```
pub fn sec_trie_root<H, S, I, A, B>(input: I) -> H::Out
where
	I: IntoIterator<Item = (A, B)>,
	A: AsRef<[u8]> + Debug,
	B: AsRef<[u8]> + Debug,
	H: Hasher,
	H::Out: Ord,
	S: TrieStream,
{
	trie_root::<H, S, _, _, _>(input.into_iter().map(|(k, v)| (H::hash(k.as_ref()), v)))
}

/// Takes a slice of key/value tuples where the key is a slice of nibbles
/// and encodes it into the provided `Stream`.
// pub fn build_trie<H, S, A, B>(input: &[(A, B)], cursor: usize, stream: &mut S)
fn build_trie<H, S, A, B>(input: &[(A, B)], cursor: usize, stream: &mut S)
where
	A: AsRef<[u8]> + Debug,
	B: AsRef<[u8]> + Debug,
	H: Hasher,
	S: TrieStream,
{
	match input.len() {
		// No input, just append empty data.
		0 => {
			// println!("[build_trie] no input; appending empty, cursor={}, stream={:?}", cursor, stream.as_raw());
			stream.append_empty_data()
		},
		// Leaf node; append the remainder of the key and the value. Done.
		1 => {
			// println!("[build_trie] appending leaf, cursor={}, stream={:?}, partial key={:?}", cursor, stream.as_raw(), &input[0].0.as_ref()[cursor..]);
			// stream.append_leaf::<H>(&input[0].0.as_ref()[cursor..], &input[0].1.as_ref() )
			stream.append_leaf(&input[0].0.as_ref()[cursor..], &input[0].1.as_ref() )
		},
		// We have multiple items in the input. We need to figure out if we
		// should add an extension node or a branch node.
		_ => {
			let (key, value) = (&input[0].0.as_ref(), input[0].1.as_ref());
			// Count the number of nibbles in the other elements that are
			// shared with the first key.
			// e.g. input = [ [1'7'3'10'12'13], [1'7'3'], [1'7'7'8'9'] ] => [1'7'] is common => 2
			let shared_nibble_count = input.iter().skip(1).fold(key.len(), |acc, &(ref k, _)| {
				cmp::min( shared_prefix_len(key, k.as_ref()), acc )
			});
			// Add an extension node if the number of shared nibbles is greater
			// than what we saw on the last call (`cursor`): append the new part
			// of the path then recursively append the remainder of all items
			// who had this partial key.
			if shared_nibble_count > cursor {
				// println!("[build_trie] appending ext and recursing, cursor={}, stream={:?}, partial key={:?}", cursor, stream.as_raw(), &key[cursor..shared_nibble_count]);
				stream.append_extension(&key[cursor..shared_nibble_count]);
				build_trie_trampoline::<H, _, _, _>(input, shared_nibble_count, stream);
				// println!("[build_trie] returning after recursing, cursor={}, stream={:?}, partial key={:?}", cursor, stream.as_raw(), &key[cursor..shared_nibble_count]);
				return;
			}
			// Add a branch node because the path is as long as it gets. The branch
			// node has 17 entries, one for each possible nibble + 1 for data.
			stream.begin_branch();
			// println!("[build_trie] started branch node, cursor={}, stream={:?}", cursor, stream.as_raw());
			// If the length of the first key is equal to the current cursor, move
			// to next element.
			let mut begin = { if cursor == key.len() {1} else {0} };
			// Fill in each slot in the branch node: an empty node if the slot
			// is unoccupied, otherwise recurse and add more nodes.
			for i in 0..16 {
				// If we've reached the end of our input, fast-forward to the
				// end filling in the slots with empty nodes. The input is sorted
				// so we know there are no more elements we need to ponder.
				if begin >= input.len() {
					for _ in i..16 {
						// println!("[build_trie] branch slot {}; fast forward, stream={:?}", i, stream.as_raw());
						stream.append_empty_data();
					}
					break;
				}
				// Count how many successive elements have same next nibble.
				let shared_nibble_count = input[begin..].iter()
					.take_while(|(k, _)| k.as_ref()[cursor] == i)
					.count();
				match shared_nibble_count {
					// If nothing is shared we're at the end of the path. Append
					// an empty node (and we'll append the value in the 17th slot
					// at the end of the method call).
					0 => stream.append_empty_data(),
					// If at least one successive element has the same nibble,
					// recurse and add more nodes.
					_ => {
						// println!("[build_trie] branch slot {}; recursing with cursor={}, begin={}, shared nibbles={}, input={:?}", i, cursor, begin, shared_nibble_count, &input[begin..(begin + shared_nibble_count)]);
						build_trie_trampoline::<H, S, _, _>(&input[begin..(begin + shared_nibble_count)], cursor + 1, stream);
					}
				}
				begin += shared_nibble_count;
			}
			// println!("[build_trie] ending branch node, cursor={}, stream={:?}", cursor, stream.as_raw());

			if cursor == key.len() {
				// println!("[build_trie] branch slot 17; cursor={}, appending value {:?}", cursor, value);
				stream.append_value(value);
			} else {
				// println!("[build_trie] branch slot 17; no value; cursor={}", cursor);
				stream.append_empty_data();
			}
		}
	}
}

fn build_trie_trampoline<H, S, A, B>(input: &[(A, B)], cursor: usize, stream: &mut S)
where
	A: AsRef<[u8]> + Debug,
	B: AsRef<[u8]> + Debug,
	H: Hasher,
	S: TrieStream,
{
	let mut substream = S::new();
	build_trie::<H, _, _, _>(input, cursor, &mut substream);
	stream.append_substream::<H>(substream);
}

#[cfg(test)]
mod tests {
	use super::{trie_root, sec_trie_root, shared_prefix_len};
	use super::unhashed_trie;
	use keccak_hasher::KeccakHasher;
	use triestream::{RlpTrieStream, CodecTrieStream};
	use parity_codec::{Encode, Compact};

	fn to_compact(num: u8) -> u8 {
		Compact(num).encode()[0]
	}

	#[test]
	fn sec_trie_root_works() {
		let v = vec![
			("doe", "reindeer"),
			("dog", "puppy"),
			("dogglesworth", "cat"),
		];
		assert_eq!(
			sec_trie_root::<KeccakHasher, RlpTrieStream, _, _, _>(v.clone()),
			"d4cd937e4a4368d7931a9cf51686b7e10abb3dce38a39000fd7902a092b64585".into(),
		);
	}

	#[test]
	fn trie_root_works() {
		let v = vec![
			("doe", "reindeer"),
			("dog", "puppy"),
			("dogglesworth", "cat"),
		];
		assert_eq!(
			trie_root::<KeccakHasher, RlpTrieStream, _, _, _>(v),
			"8aad789dff2f538bca5d8ea56e8abe10f4c7ba3a5dea95fea4cd6e7c3a1168d3".into()
		);
		assert_eq!(
			trie_root::<KeccakHasher, RlpTrieStream, _, _, _>(vec![
				(b"A", b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" as &[u8])
			]),
			"d23786fb4a010da3ce639d66d5e904a11dbc02746d1ce25029e53290cabf28ab".into()
		);
	}

	#[test]
	fn test_triehash_out_of_order() {
		assert!(trie_root::<KeccakHasher, RlpTrieStream, _, _, _>(vec![
			(vec![0x01u8, 0x23], vec![0x01u8, 0x23]),
			(vec![0x81u8, 0x23], vec![0x81u8, 0x23]),
			(vec![0xf1u8, 0x23], vec![0xf1u8, 0x23]),
		]) ==
		trie_root::<KeccakHasher, RlpTrieStream, _, _, _>(vec![
			(vec![0x01u8, 0x23], vec![0x01u8, 0x23]),
			(vec![0xf1u8, 0x23], vec![0xf1u8, 0x23]), // last two tuples are swapped
			(vec![0x81u8, 0x23], vec![0x81u8, 0x23]),
		]));
	}

	#[test]
	fn test_shared_prefix() {
		let a = vec![1,2,3,4,5,6];
		let b = vec![4,2,3,4,5,6];
		assert_eq!(shared_prefix_len(&a, &b), 0);
	}

	#[test]
	fn test_shared_prefix2() {
		let a = vec![1,2,3,3,5];
		let b = vec![1,2,3];
		assert_eq!(shared_prefix_len(&a, &b), 3);
	}

	#[test]
	fn test_shared_prefix3() {
		let a = vec![1,2,3,4,5,6];
		let b = vec![1,2,3,4,5,6];
		assert_eq!(shared_prefix_len(&a, &b), 6);
	}

	#[test]
	fn learn_rlp_trie_empty() {
		let input: Vec<(&[u8], &[u8])> = vec![];
		let trie = unhashed_trie::<KeccakHasher, RlpTrieStream, _, _, _>(input);
		println!("[learn_rlp_trie_empty] 1st byte of trie:\n{:#010b}\n trie: {:#x?}", trie[0], trie );
		assert_eq!(trie, vec![0x80]);
	}

	#[test]
	fn learn_rlp_trie_single_item() {
		let input: Vec<(&[u8], &[u8])> = vec![(&[0x13], &[0x14])];
		let trie = unhashed_trie::<KeccakHasher, RlpTrieStream, _, _, _>(input);
		println!("[learn_rlp_trie_single_item] 1st byte of trie:\n{:#010b}\n trie: {:#x?}", trie[0], trie );
		assert_eq!(trie, vec![0xc4, 0x82, 0x20, 0x13, 0x14]);
		// The key, 0x13, as nibbles: [ 0x1, 0x3 ]
		// build_trie will call append_leaf with k/v: [ 0x1, 0x3 ], [0x14]
		// 	append_leaf will call rlp begin_list(2)
		// 		begin_list adds 0 to buffer - modified later when list is closed
		//	key is hpe'd: even length, leaf (terminated) => high nibble sets termination bit, low nibble is zero => 0b0010_0000 => 0x20 => 32
		// 	append_iter() is called with hpe byte + key byte => 0x20, 0x13; adds 0x80 + length of items (2) => 0x82
		//	buffer is now: 0, 0x82, 0x20, 0x13, 0x14
		//	append() adds the value bytes => 0x14 and closes the list: 0xc0 + length of payload => 0xc0 + 4
		// final buffer: 0xc4 0x82 0x20 0x13 0x14
	}

	#[test]
	fn learn_rlp_trie_single_item2() {
		let input = vec![(
			vec![0x12, 0x12, 0x12, 0x12, 0x13, 0x13], 	// key
			vec![0xff, 0xfe, 0xfd, 0xfc]				// val
		)];
		let trie = unhashed_trie::<KeccakHasher, RlpTrieStream, _, _, _>(input);
		// println!("[learn_rlp_trie_single_item] 1st byte of trie:\n{:#010b}\n trie: {:#x?}", trie[0], trie );
		assert_eq!(trie, vec![
			0xc0 + 13,	// list marker + 13 bytes long payload
			0x80 + 7,	// value marker + 7 bytes long payload
			0x20, 		// HPE byte
			0x12, 0x12, 0x12, 0x12, 0x13, 0x13,
			0x80 + 4, 	// value marker + 4 bytes long payload
			0xff, 0xfe, 0xfd, 0xfc
		]);
	}

	#[test]
	fn learn_codec_trie_empty() {
		let input: Vec<(&[u8], &[u8])> = vec![];
		let trie = unhashed_trie::<KeccakHasher, CodecTrieStream, _, _, _>(input);
		println!("trie: {:#x?}", trie);
		assert_eq!(trie, vec![0x0]);
	}

	#[test]
	fn learn_codec_trie_single_tuple() {
		let input = vec![
			(vec![0xaa], vec![0xbb])
		];
		let trie = unhashed_trie::<KeccakHasher, CodecTrieStream, _, _, _>(input);
		println!("trie: {:#x?}", trie);

		let mut expected: Vec<u8> = vec![];
		expected.push(0b1010_0000);
		expected.extend(vec![0xaau8].encode());
		expected.extend(vec![0xbbu8].encode());
		assert_eq!(trie, expected);

		assert_eq!(trie, vec![
			0b1010_0000,			// leaf
			to_compact(1),			// length
			0xaa,					// key
			to_compact(1),			// length
			0xbb					// value
		]);
	}

	#[test]
	fn learn_codec_trie_two_tuples_disjoint_keys() {
		let input = vec![(&[0x48, 0x19], &[0xfe]), (&[0x13, 0x14], &[0xff])];
		let trie = unhashed_trie::<KeccakHasher, CodecTrieStream, _, _, _>(input);
		println!("trie: {:#x?}", trie);

		let mut ex = Vec::<u8>::new();
		ex.push(0b0100_0000);							// branch
		ex.push(0);										// slot 0
		let mut sub = vec![0b1011_0000 + 3];			// slot 1 LEAF; 176 + 3, i.e. the first of the remaining key nibbles (3'1'4')
		sub.extend(vec![0x14u8].encode());				// key
		sub.extend(vec![0xffu8].encode());				// value
		ex.extend( sub.encode() );
		ex.push(0);										// slot 2, 3
		ex.push(0);
		let mut sub = vec![0b1011_0000 + 8];			// slot 4 LEAF; remaining nibbles: 8'1'9'; odd, so 8 goes into lower nibble
		sub.extend(vec![0x19u8].encode());				// key
		sub.extend(vec![0xfeu8].encode());				// value
		ex.extend( sub.encode() );
		ex.extend(vec![0u8,0,0,0,0,0,0,0,0,0,0, 0]);	// slots 5..15 + value slot

		assert_eq!(trie, ex);

		assert_eq!(trie, vec![
										// <–– TODO: why is there no length here?
			0b0100_0000,				// BRANCH
			0x00, 						// slot 0
			to_compact(0x05),			// 5 – length in bytes of the following node
			0b1011_0000 + 3, 			// slot 1 LEAF; 176 + 3, i.e. the first of the remaining key nibbles (3'1'4')
				to_compact(0x01), 		// key length: 1 bytes
				0x14,					// key
				to_compact(0x01), 		// key length: 1 bytes
				0xff,					// value
			0x00, 0x00, 				// slots 2,3
			to_compact(0x05),			// 5 – length in bytes of the following node
			0b1011_0000 + 8, 			// slot 4 LEAF; remaining nibbles: 8'1'9'; odd, so 8 goes into lower nibble
				to_compact(0x01), 		// key length: 1 bytes
				0x19,					// key
				to_compact(0x01), 		// value length: 1 bytes
				0xfe,					// value
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // slots 5..15
			0x00, 						// slot 16,
		]);
	}

	#[test]
	fn learn_codec_trie_single_item() {
		let input: Vec<(&[u8], &[u8])> = vec![(&[0x13], &[0x14])];
		let trie = unhashed_trie::<KeccakHasher, CodecTrieStream, _, _, _>(input);
		println!("[learn_codec_trie_single_item] 1st byte of trie:\n{:#010b}\n trie: {:#x?}", trie[0], trie );
		assert_eq!(trie, vec![
			0b10_10_0000, 			// variant: leaf, even payload length
			to_compact(0x01), 		// key length: 1 bytes
			0x13,					// key
			to_compact(0x01), 		// value length: 1 bytes
			0x14					// value
		]);

		let input = vec![(
			vec![0x12, 0x12, 0x12, 0x12, 0x13],	// key
			vec![0xff, 0xfe, 0xfd, 0xfc]		// val
		)];
		let trie = unhashed_trie::<KeccakHasher, CodecTrieStream, _, _, _>(input);
		assert_eq!(trie, vec![
			0b10_10_0000, 			// variant: leaf, even payload length
			to_compact(0x05), 		// key length: 5 bytes
			0x12, 0x12, 0x12, 0x12, 0x13,
			to_compact(0x04), 		// value length: 4 bytes
			0xff, 0xfe, 0xfd, 0xfc
		]);
	}

	#[test]
	fn learn_rlp_trie_full_example() {
		let input = vec![
			(vec![0xa7, 0x11, 0x35, 0x5], vec![45]),
			(vec![0xa7, 0x7d, 0x33, 0x7], vec![1]),
			(vec![0xa7, 0xf9, 0x36, 0x5], vec![11]),
			(vec![0xa7, 0x7d, 0x39, 0x7], vec![12]),
		];
		/*
		Expected trie:
			Extension, 0xa7
			Branch
				1: Leaf ([0x01, 0x35, 0x5], 45)
				7: Extension, d3
					Branch
						3: Leaf ([0x03, 0x07], 1)
						9: Leaf ([0x09, 0x07], 12)
				f: Leaf (0x09, 0x36, 0x5, 11)
		*/
		let rlp_trie = unhashed_trie::<KeccakHasher, RlpTrieStream, _, _, _>(input);
		println!("rlp trie: {:#x?}", rlp_trie);
		// TODO: finish
		// assert_eq!(rlp_trie, vec![
		// 	0xc0 + 36,
		// 	0x80 + 2,
		// 	0b0000_0000,	// HPE flag-byte
		// 	0xa7,			// partial key; end ext
		// 	0x80 + 32, 		// begin_list(17) - why 32? hash len?
		// 	0x80,			// slot 0: empty
		// 	0xc0 + 7,		// slot 1: start list(2) to build leaf
		// 	0x80 + 3,		// value marker + length
		// 	0x31, 			// HPE byte 0b00_11_0001 (leaf, odd, 1 in lower nibble)
		// 	0x35, 0x05,		// rest of key,
		// 	0x80 + 1,		// value marker
		// 	45,				// value
		// 	0x80,			// slot 2: empty
		// 	0x80,			// slot 3: empty
		// 	0x80,			// slot 4: empty
		// 	0x80,			// slot 5: empty
		// 	0x80,			// slot 6: empty
		// 	0xc0 + 0,		// slot 7: extension, begin_list(2)
		// 	0b0000_0000,	// HPE flag-byte
		// 	0x80 + 2,		// value marker + length
		// 	0xd3,			// partial key; end ext
		// 	0xc0 + 0		// branch node; begin list
		// … … …
		// ]);

	}

	#[test]
	fn learn_codec_trie_full_example() {
		let input = vec![
			(vec![0xa7, 0x11, 0x35, 0x5], vec![45]),
			(vec![0xa7, 0x7d, 0x33, 0x7], vec![1]),
			(vec![0xa7, 0xf9, 0x36, 0x5], vec![11]),
			(vec![0xa7, 0x7d, 0x39, 0x7], vec![12]),
		];
		/*
		Expected trie:
			Extension, 0xa7
			Branch
				1: Leaf ([0x01, 0x35, 0x5], 45)
				7: Extension, d3
					Branch
						3: Leaf ([0x03, 0x07], 1)
						9: Leaf ([0x09, 0x07], 12)
				f: Leaf (0x09, 0x36, 0x5, 11)
		*/
		let codec_trie = unhashed_trie::<KeccakHasher, CodecTrieStream, _, _, _>(input.clone());
		println!("codec trie: {:#x?}", codec_trie);

		assert_eq!(codec_trie, vec![
			0x80,				// 0b10000000 => extension
			to_compact(0x1),	// length 1
			0xa7,				// payload: a7
			to_compact(62),		// length 62 bytes
			0x40,				// Branch node: 0b01_00_0000
			0x0,				// slot 0: empty node
			to_compact(0x6),	// slot 1: 6 bytes follow
			0xb1,				// 0xb1 == 177 == 0b1011_0001 => 0b10_11_xxxx, leaf, odd length + 0001
			to_compact(0x2),	// length: 2 bytes
			0x35,				// key payload
			0x5,
			to_compact(0x1),	// value length: 1 byte
			0x2d,				// value: 45; 12th byte, ends slot 1
			0x0,				// slot 2
			0x0,				// slot 3
			0x0,				// slot 4
			0x0,				// slot 5
			0x0,				// slot 6
			to_compact(32),		// slot 7; item of length 32
			0x80,				// extension node, 0b10000000
			to_compact(0x1),	// key length: 1 byte
			0xd3,				// key payload, 0xd3
			to_compact(28),		// item of length 28
			0x40,				// Branch node: 0b01_00_0000
			0x0,				// slot 0
			0x0,				// slot 1
			0x0,				// slot 2
			to_compact(0x5),	// slot 3, item of length 5
			0xa0,				// payload, 0b1010_0000: leaf node, even length
			to_compact(0x1),	// key length: 1 byte
			0x7,				// partial key payload: 7
			to_compact(0x1),	// value length: 1 byte
			0x1,				// value payload: 1
			0x0,				// slot 4
			0x0,				// slot 5
			0x0,				// slot 6
			0x0,				// slot 7
			0x0,				// slot 8
			to_compact(0x5),	// slot 9,  item of length 11
			0xa0,				// payload, 0b1010_0000: lead node, even length
			to_compact(0x1),	// key length 1 byte
			0x7,				// key payload: 7
			to_compact(0x1),	// value length: 1 byte
			0xc,				// value payload: 12
			0x0,				// slot 11
			0x0,				// slot 12
			0x0,				// slot 13
			0x0,				// slot 14
			0x0,				// slot 15; end second branch node
			0x0,				// slot 16; second branch value slot
			0x0,				// slot 8 (first branch)
			0x0,				// slot 9
			0x0,				// slot 10
			0x0,				// slot 11
			0x0,				// slot 12
			0x0,				// slot 13
			0x0,				// slot 14
			0x0,				// slot 15
			to_compact(0x6),	// slot 16; first branch value slot; item of length 12
			0xb9,				// 0xb9 == 185 == 0b1011_1001 => Leaf node, odd number, partial key payload = 9
			to_compact(0x2),	// length: 2 bytes
			0x36,				// payload: 0x36, 0x5
			0x5,
			to_compact(0x1),	// length: 1 byte
			0xb,				// value: 11
			0x0
		]);
	}
}
