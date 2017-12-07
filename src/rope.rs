#![allow(dead_code)]

use std;
use std::sync::Arc;

use arrayvec::ArrayVec;

use smallvec::Array;

use slice::RopeSlice;
use small_string::SmallString;
use small_string_utils::{char_pos_to_byte_pos, split_string_near_byte, LineBreakIter,
                         fix_grapheme_seam};


// Internal node min/max values.
const MAX_CHILDREN: usize = 16;
const MIN_CHILDREN: usize = MAX_CHILDREN / 2;

// Leaf node min/max values.
const MAX_BYTES: usize = 334;
const MIN_BYTES: usize = MAX_BYTES / 2;

#[derive(Copy, Clone)]
pub(crate) struct BackingArray([u8; MAX_BYTES]);
unsafe impl Array for BackingArray {
    type Item = u8;
    fn size() -> usize {
        MAX_BYTES
    }
    fn ptr(&self) -> *const u8 {
        &self.0[0]
    }
    fn ptr_mut(&mut self) -> *mut u8 {
        &mut self.0[0]
    }
}

// Type alias used for char count, grapheme count, line count, etc. storage
// in nodes.
pub type Count = u32;

//=============================================================

#[derive(Debug, Clone)]
pub struct Rope {
    pub(crate) root: Arc<Node>,
}

impl Rope {
    /// Creates an empty Rope.
    pub fn new() -> Rope {
        use std::mem::size_of;
        println!("Node size: {:?}", size_of::<Node>());
        Rope { root: Arc::new(Node::new()) }
    }

    /// Total number of bytes in the Rope.
    pub fn len(&self) -> usize {
        self.root.text_info().bytes as usize
    }

    /// Total number of chars in the Rope.
    pub fn char_count(&self) -> usize {
        self.root.text_info().chars as usize
    }

    /// Total number of lines in the Rope.
    pub fn line_count(&self) -> usize {
        self.root.text_info().line_breaks as usize + 1
    }

    /// Returns the char index of the given byte.
    pub fn byte_to_char(&self, byte_idx: usize) -> usize {
        let _ = byte_idx;
        unimplemented!()
    }

    /// Returns the line index of the given byte.
    pub fn byte_to_line(&self, byte_idx: usize) -> usize {
        let _ = byte_idx;
        unimplemented!()
    }

    /// Returns the byte index of the given char.
    pub fn char_to_byte(&self, char_idx: usize) -> usize {
        self.root.char_index_to_byte_index(char_idx as Count) as usize
    }

    /// Returns the line index of the given char.
    pub fn char_to_line(&self, char_idx: usize) -> usize {
        let _ = char_idx;
        unimplemented!()
    }

    /// Returns the byte index of the start of the given line.
    pub fn line_to_byte(&self, line_idx: usize) -> usize {
        let _ = line_idx;
        unimplemented!()
    }

    /// Returns the char index of the start of the given line.
    pub fn line_to_char(&self, line_idx: usize) -> usize {
        let _ = line_idx;
        unimplemented!()
    }

    /// Returns an immutable slice of the Rope in the char range `start..end`.
    pub fn slice<'a>(&'a self, start: usize, end: usize) -> RopeSlice<'a> {
        RopeSlice::new_from_node(&self.root, start, end)
    }

    /// Returns the entire text of the Rope as a newly allocated String.
    pub fn to_string(&self) -> String {
        use iter::RopeChunkIter;
        let mut text = String::new();
        for chunk in RopeChunkIter::new(self) {
            text.push_str(chunk);
        }
        text
    }

    /// Inserts the given text at char index `char_idx`.
    pub fn insert(&mut self, char_idx: usize, text: &str) {
        let root = Arc::make_mut(&mut self.root);

        // Do the insertion
        let (residual, seam) = root.insert(char_idx as Count, text);

        // Handle residual node, if any.
        if let Some(r_node) = residual {
            let mut l_node = Node::Empty;
            std::mem::swap(&mut l_node, root);

            let mut info = ArrayVec::new();
            info.push(l_node.text_info());
            info.push(r_node.text_info());

            let mut children = ArrayVec::new();
            children.push(Arc::new(l_node));
            children.push(Arc::new(r_node));

            *root = Node::Internal {
                info: info,
                children: children,
            };
        }

        // Handle seam, if any.
        if let Some(byte_pos) = seam {
            root.fix_grapheme_seam(byte_pos);
        }
    }

    /// Removes the text in char range `start..end`.
    pub fn remove(&mut self, start: usize, end: usize) {
        let _ = (start, end);
        unimplemented!()
    }

    /// Splits the Rope at char index `split_char_idx`.
    ///
    /// The left side of the split remians in this Rope, and
    /// the right side is returned as a new Rope.
    pub fn split(&mut self, split_char_idx: usize) -> Rope {
        let _ = split_char_idx;
        unimplemented!()
    }

    /// Appends a Rope to the end of this one, consuming the other Rope.
    pub fn append(&mut self, other: Rope) {
        let _ = other;
        unimplemented!()
    }

    //--------------

    /// Debugging tool to make sure that all of the meta-data of the
    /// tree is consistent with the actual data.
    pub(crate) fn verify_integrity(&self) {
        self.root.verify_integrity();
    }
}

//=============================================================

#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) struct TextInfo {
    pub(crate) bytes: Count,
    pub(crate) chars: Count,
    pub(crate) line_breaks: Count,
}

impl TextInfo {
    fn new() -> TextInfo {
        TextInfo {
            bytes: 0,
            chars: 0,
            line_breaks: 0,
        }
    }

    fn from_str(text: &str) -> TextInfo {
        TextInfo {
            bytes: text.len() as Count,
            chars: text.chars().count() as Count,
            line_breaks: LineBreakIter::new(text).count() as Count,
        }
    }

    fn combine(&self, other: &TextInfo) -> TextInfo {
        TextInfo {
            bytes: self.bytes + other.bytes,
            chars: self.chars + other.chars,
            line_breaks: self.line_breaks + other.line_breaks,
        }
    }
}

trait TextInfoArray {
    fn combine(&self) -> TextInfo;
    fn search_combine<F: Fn(&TextInfo) -> bool>(&self, pred: F) -> (usize, TextInfo);
}

impl TextInfoArray for [TextInfo] {
    fn combine(&self) -> TextInfo {
        self.iter().fold(TextInfo::new(), |a, b| a.combine(b))
    }

    fn search_combine<F: Fn(&TextInfo) -> bool>(&self, pred: F) -> (usize, TextInfo) {
        let mut accum = TextInfo::new();
        for (idx, inf) in self.iter().enumerate() {
            if pred(&accum.combine(inf)) {
                return (idx, accum);
            } else {
                accum = accum.combine(inf);
            }
        }
        panic!("Predicate is mal-formed and never evaluated true.")
    }
}

//=============================================================

#[derive(Debug, Clone)]
pub(crate) enum Node {
    Empty,
    Leaf(SmallString<BackingArray>),
    Internal {
        info: ArrayVec<[TextInfo; MAX_CHILDREN]>,
        children: ArrayVec<[Arc<Node>; MAX_CHILDREN]>,
    },
}

impl Node {
    fn new() -> Node {
        Node::Empty
    }

    pub(crate) fn text_info(&self) -> TextInfo {
        match self {
            &Node::Empty => TextInfo::new(),
            &Node::Leaf(ref text) => TextInfo::from_str(text),
            &Node::Internal { ref info, .. } => {
                info.iter().fold(TextInfo::new(), |a, b| a.combine(b))
            }
        }
    }

    fn char_index_to_byte_index(&self, char_pos: Count) -> Count {
        match self {
            &Node::Empty => 0,
            &Node::Leaf(ref text) => char_pos_to_byte_pos(text, char_pos as usize) as Count,
            &Node::Internal {
                ref info,
                ref children,
            } => {
                let (child_i, acc_info) = info.search_combine(|inf| char_pos <= inf.chars);

                acc_info.bytes +
                    children[child_i].char_index_to_byte_index(char_pos - acc_info.chars)
            }
        }
    }

    /// Inserts the text at the given char index.
    ///
    /// Returns a right-side residual node if the insertion wouldn't fit
    /// within this node.  Also returns the byte position where there may
    /// be a grapheme seam to fix, if any.
    ///
    /// TODO: handle the situation where what's being inserted is larger
    /// than MAX_BYTES.
    fn insert(&mut self, char_pos: Count, text: &str) -> (Option<Node>, Option<Count>) {
        match self {
            // If it's empty, turn it into a leaf
            &mut Node::Empty => {
                *self = Node::Leaf(text.into());
                return (None, None);
            }

            // If it's a leaf
            &mut Node::Leaf(ref mut cur_text) => {
                let byte_pos = char_pos_to_byte_pos(cur_text, char_pos as usize);
                let seam = if byte_pos == 0 {
                    Some(0)
                } else if byte_pos == cur_text.len() {
                    let count = (cur_text.len() + text.len()) as Count;
                    Some(count)
                } else {
                    None
                };

                cur_text.insert_str(byte_pos, text);

                if cur_text.len() <= MAX_BYTES {
                    return (None, seam);
                } else {
                    let split_pos = cur_text.len() - (cur_text.len() / 2);
                    let right_text = split_string_near_byte(cur_text, split_pos);
                    if right_text.len() > 0 {
                        cur_text.shrink_to_fit();
                        return (Some(Node::Leaf(right_text)), seam);
                    } else {
                        // Leaf couldn't be validly split, so leave it oversized
                        return (None, seam);
                    }
                }
            }

            // If it's internal, things get a little more complicated
            &mut Node::Internal {
                ref mut info,
                ref mut children,
            } => {
                // Find the child to traverse into along with its starting char
                // offset.
                let (child_i, start_info) = info.search_combine(|inf| char_pos <= inf.chars);
                let start_char = start_info.chars;

                // Navigate into the appropriate child
                let (residual, child_seam) =
                    Arc::make_mut(&mut children[child_i]).insert(char_pos - start_char, text);
                info[child_i] = children[child_i].text_info();

                // Calculate the seam offset relative to this node
                let seam = child_seam.map(|byte_pos| byte_pos + start_info.bytes);

                // Handle the new node, if any.
                if let Some(r_node) = residual {
                    // The new node will fit as a child of this node
                    if children.len() < MAX_CHILDREN {
                        info.insert(child_i + 1, r_node.text_info());
                        children.insert(child_i + 1, Arc::new(r_node));
                        return (None, seam);
                    }
                    // The new node won't fit!  Must split.
                    else {
                        let (extra_info, extra_child) = if child_i < (children.len() - 1) {
                            let extra_info = info.pop().unwrap();
                            let extra_child = children.pop().unwrap();
                            info.insert(child_i + 1, r_node.text_info());
                            children.insert(child_i + 1, Arc::new(r_node));
                            (extra_info, extra_child)
                        } else {
                            (r_node.text_info(), Arc::new(r_node))
                        };

                        let mut r_info = ArrayVec::new();
                        let mut r_children = ArrayVec::new();

                        let r_count = (children.len() + 1) / 2;
                        let l_count = (children.len() + 1) - r_count;

                        for _ in l_count..children.len() {
                            r_info.push(info.remove(l_count));
                            r_children.push(children.remove(l_count));
                        }
                        r_info.push(extra_info);
                        r_children.push(extra_child);

                        return (
                            Some(Node::Internal {
                                info: r_info,
                                children: r_children,
                            }),
                            seam,
                        );
                    }
                } else {
                    // No new node.  Easy.
                    return (None, seam);
                }
            }
        }
    }

    //-----------------------------------------

    /// Debugging tool to make sure that all of the meta-data of the
    /// tree is consistent with the actual data.
    fn verify_integrity(&self) {
        match self {
            &Node::Empty => {}
            &Node::Leaf(_) => {}
            &Node::Internal {
                ref info,
                ref children,
            } => {
                assert_eq!(info.len(), children.len());
                for (inf, child) in info.iter().zip(children.iter()) {
                    assert_eq!(*inf, child.text_info());
                    child.verify_integrity();
                }
            }
        }
    }

    /// Checks to make sure that a boundary between leaf nodes (given as a byte
    /// position in the rope) doesn't split a grapheme, and fixes it if it does.
    ///
    /// Note: panics if the given byte position is not on the boundary between
    /// two leaf nodes.
    fn fix_grapheme_seam(&mut self, byte_pos: Count) -> Option<&mut SmallString<BackingArray>> {
        match self {
            &mut Node::Empty => return None,

            &mut Node::Leaf(ref mut text) => {
                if byte_pos == 0 || byte_pos == text.len() as Count {
                    Some(text)
                } else {
                    panic!("Byte position given is not on a leaf boundary.")
                }
            }

            &mut Node::Internal {
                ref mut info,
                ref mut children,
            } => {
                if byte_pos == 0 {
                    // Special-case 1
                    return Arc::make_mut(&mut children[0]).fix_grapheme_seam(byte_pos);
                } else if byte_pos == info.combine().bytes {
                    // Special-case 2
                    return Arc::make_mut(children.last_mut().unwrap())
                        .fix_grapheme_seam(info.last().unwrap().bytes);
                } else {
                    // Find the child to navigate into
                    let (child_i, start_info) = info.search_combine(|inf| byte_pos <= inf.bytes);
                    let start_byte = start_info.bytes;

                    let pos_in_child = byte_pos - start_byte;
                    let child_len = info[child_i].bytes;

                    if pos_in_child == 0 || pos_in_child == child_len {
                        // Left or right edge, get neighbor and fix seam
                        let ((split_l, split_r), child_l_i) = if pos_in_child == 0 {
                            (children.split_at_mut(child_i), child_i - 1)
                        } else {
                            (children.split_at_mut(child_i + 1), child_i)
                        };
                        let left_child = Arc::make_mut(split_l.last_mut().unwrap());
                        let right_child = Arc::make_mut(split_r.first_mut().unwrap());
                        fix_grapheme_seam(
                            left_child.fix_grapheme_seam(info[child_l_i].bytes).unwrap(),
                            right_child.fix_grapheme_seam(0).unwrap(),
                        );
                        left_child.fix_info_right();
                        right_child.fix_info_left();
                        info[child_l_i] = left_child.text_info();
                        info[child_l_i + 1] = right_child.text_info();
                        return None;
                    } else {
                        // Internal to child
                        return Arc::make_mut(&mut children[child_i]).fix_grapheme_seam(
                            pos_in_child,
                        );
                    }
                }
            }
        }
    }

    /// Updates the tree meta-data down the left side of the tree.
    fn fix_info_left(&mut self) {
        match self {
            &mut Node::Empty => {}
            &mut Node::Leaf(_) => {}
            &mut Node::Internal {
                ref mut info,
                ref mut children,
            } => {
                let left = Arc::make_mut(children.first_mut().unwrap());
                left.fix_info_left();
                *info.first_mut().unwrap() = left.text_info();
            }
        }
    }

    /// Updates the tree meta-data down the right side of the tree.
    fn fix_info_right(&mut self) {
        match self {
            &mut Node::Empty => {}
            &mut Node::Leaf(_) => {}
            &mut Node::Internal {
                ref mut info,
                ref mut children,
            } => {
                let right = Arc::make_mut(children.last_mut().unwrap());
                right.fix_info_right();
                *info.last_mut().unwrap() = right.text_info();
            }
        }
    }
}
