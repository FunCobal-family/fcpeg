use std::cell::RefCell;
use std::fmt::*;
use std::io::*;
use std::io::Write;
use std::rc::Rc;

use rustnutlib::*;
use rustnutlib::console::*;

use uuid::Uuid;

pub enum TreeLog {
    Unknown {},
    CharacterPositionNotFound { uuid: Uuid },
    ElementNotNode { uuid: Uuid },
    ElementNotLeaf { uuid: Uuid },
    NodeChildNotFound { parent_uuid: Uuid, index: usize },
    ReflectableChildNotFound { parent_uuid: Uuid, index: usize },
}

impl ConsoleLogger for TreeLog {
    fn get_log(&self) -> ConsoleLog {
        return match self {
            TreeLog::Unknown {} => log!(Error, "unknown error"),
            TreeLog::CharacterPositionNotFound { uuid } => log!(Error, "character position not found", format!("uuid:\t{}", uuid)),
            TreeLog::ElementNotNode { uuid } => log!(Error, "element not node", format!("uuid:\t{}", uuid)),
            TreeLog::ElementNotLeaf { uuid } => log!(Error, "element not leaf", format!("uuid:\t{}", uuid)),
            TreeLog::NodeChildNotFound { parent_uuid, index } => log!(Error, "node child not found", format!("parent:\t{}", parent_uuid), format!("index:\t{}", index)),
            TreeLog::ReflectableChildNotFound { parent_uuid, index } => log!(Error, "reflectable child not found", format!("parent:\t{}", parent_uuid), format!("index:\t{}", index)),
        };
    }
}

#[derive(Clone, PartialEq)]
pub struct CharacterPosition {
    pub file_path: Option<String>,
    pub index: usize,
    pub line: usize,
    pub column: usize,
}

impl CharacterPosition {
    pub fn new(file_path: Option<String>, index: usize, line: usize, column: usize) -> CharacterPosition {
        return CharacterPosition {
            file_path: file_path,
            index: index,
            line: line,
            column: column,
        };
    }

    pub fn get_empty() -> CharacterPosition {
        return CharacterPosition {
            file_path: None,
            index: 0,
            line: 0,
            column: 0,
        };
    }
}

impl Display for CharacterPosition {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let file_path_text = match self.file_path.clone() {
            Some(path) => format!("{}:", path),
            None => String::new(),
        };

        return write!(f, "{}{}:{}", file_path_text, self.line + 1, self.column + 1);
    }
}

#[derive(Clone, PartialEq)]
pub enum ASTReflectionStyle {
    // note: AST に反映される
    Reflection(String),
    // note: AST に反映されない
    NoReflection,
    Expansion,
}

impl ASTReflectionStyle {
    // todo: config データの扱いを修正
    pub fn from_config(reverse_ast_reflection: bool, is_reflectable: bool, elem_name: String) -> ASTReflectionStyle {
        return if is_reflectable {
            if reverse_ast_reflection {
                ASTReflectionStyle::Reflection(elem_name)
            } else {
                ASTReflectionStyle::NoReflection
            }
        } else {
            if reverse_ast_reflection {
                ASTReflectionStyle::NoReflection
            } else{
                ASTReflectionStyle::Reflection(elem_name)
            }
        }
    }

    pub fn is_reflectable(&self) -> bool {
        return *self != ASTReflectionStyle::NoReflection;
    }

    pub fn is_expandable(&self) -> bool {
        return *self == ASTReflectionStyle::Expansion;
    }
}

impl Display for ASTReflectionStyle {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let s = match self {
            ASTReflectionStyle::Reflection(elem_name) => format!("#{}", elem_name.clone()),
            ASTReflectionStyle::NoReflection => String::new(),
            ASTReflectionStyle::Expansion => "##".to_string(),
        };

        return write!(f, "{}", s);
    }
}

#[derive(Clone)]
pub struct SyntaxTree {
    child: SyntaxNodeChild,
}

impl SyntaxTree {
    pub fn from_node(node: SyntaxNodeChild) -> SyntaxTree {
        return SyntaxTree {
            child: node,
        };
    }

    pub fn from_node_args(subelems: Vec<SyntaxNodeChild>, ast_reflection_style: ASTReflectionStyle) -> SyntaxTree {
        return SyntaxTree {
            child: SyntaxNodeChild::Node(Box::new(SyntaxNode::new(Uuid::new_v4(), subelems, ast_reflection_style))),
        };
    }

    pub fn print(&self, ignore_hidden_elems: bool) {
        self.child.print(ignore_hidden_elems)
    }

    pub fn get_child_ref(&self) -> &SyntaxNodeChild {
        return &self.child;
    }
}

#[derive(Clone)]
pub enum SyntaxNodeChild {
    Node(Box<SyntaxNode>),
    Leaf(Box<SyntaxLeaf>),
}

impl SyntaxNodeChild {
    pub fn from_node_args(subelems: Vec<SyntaxNodeChild>, ast_reflection_style: ASTReflectionStyle) -> SyntaxNodeChild {
        return SyntaxNodeChild::Node(Box::new(SyntaxNode::new(Uuid::new_v4(), subelems, ast_reflection_style)));
    }

    pub fn from_leaf_args(pos: CharacterPosition, value: String, ast_reflection: ASTReflectionStyle) -> SyntaxNodeChild {
        return SyntaxNodeChild::Leaf(Box::new(SyntaxLeaf::new(Uuid::new_v4(), pos, value, ast_reflection)));
    }

    pub fn get_node(&self, cons: &Rc<RefCell<Console>>) -> ConsoleResult<&SyntaxNode> {
        return match self {
            SyntaxNodeChild::Node(node) => Ok(node),
            SyntaxNodeChild::Leaf(leaf) => {
                cons.borrow_mut().append_log(TreeLog::ElementNotNode {
                    uuid: leaf.uuid.clone(),
                }.get_log());

                return Err(());
            },
        };
    }

    pub fn get_leaf(&self, cons: &Rc<RefCell<Console>>) -> ConsoleResult<&SyntaxLeaf> {
        return match self {
            SyntaxNodeChild::Node(node) => {
                cons.borrow_mut().append_log(TreeLog::ElementNotLeaf {
                    uuid: node.uuid.clone(),
                }.get_log());

                return Err(());
            },
            SyntaxNodeChild::Leaf(leaf) => Ok(leaf),
        };
    }

    pub fn is_node(&self) -> bool {
        return match self {
            SyntaxNodeChild::Node(_) => true,
            _ => false,
        };
    }

    pub fn is_reflectable(&self) -> bool {
        return match self {
            SyntaxNodeChild::Node(node) => node.is_reflectable(),
            SyntaxNodeChild::Leaf(leaf) => leaf.is_reflectable(),
        };
    }

    pub fn get_ast_reflection_style(&self) -> ASTReflectionStyle {
        return match self {
            SyntaxNodeChild::Node(node) => node.ast_reflection_style.clone(),
            SyntaxNodeChild::Leaf(leaf) => leaf.ast_reflection_style.clone(),
        };
    }

    pub fn set_ast_reflection_style(&mut self, ast_reflection_style: ASTReflectionStyle) {
        match self {
            SyntaxNodeChild::Node(node) => node.ast_reflection_style = ast_reflection_style,
            SyntaxNodeChild::Leaf(leaf) => leaf.ast_reflection_style = ast_reflection_style,
        }
    }

    pub fn print(&self, ignore_hidden_elems: bool) {
        self.print_with_details(0, &mut BufWriter::new(stdout().lock()), ignore_hidden_elems)
    }

    pub fn print_with_details(&self, nest: usize, writer: &mut BufWriter<StdoutLock>, ignore_hidden_elems: bool) {
        match self {
            SyntaxNodeChild::Node(node) => node.print_with_details(nest, writer, ignore_hidden_elems),
            SyntaxNodeChild::Leaf(leaf) => leaf.print_with_details(nest, writer, ignore_hidden_elems),
        }
    }
}

#[derive(Clone)]
pub struct SyntaxNode {
    pub uuid: Uuid,
    pub subelems: Vec<SyntaxNodeChild>,
    pub ast_reflection_style: ASTReflectionStyle,
}

impl SyntaxNode {
    pub fn new(uuid: Uuid, subelems: Vec<SyntaxNodeChild>, ast_reflection_style: ASTReflectionStyle) -> SyntaxNode {
        return SyntaxNode {
            uuid: uuid,
            subelems: subelems,
            ast_reflection_style: ast_reflection_style,
        };
    }

    pub fn exists_child_node(&self, patterns: Vec<&str>) -> bool {
        return self.find_first_child_node(patterns).is_some();
    }

    pub fn filter_children(&self, f: fn(&SyntaxNodeChild) -> bool) -> Vec<&SyntaxNodeChild> {
        let mut elems = Vec::<&SyntaxNodeChild>::new();

        for each_elem in &self.subelems {
            if f(each_elem) {
                elems.push(each_elem);
            }
        }

        return elems;
    }

    pub fn get_reflectable_children(&self) -> Vec<&SyntaxNodeChild> {
        return self.filter_children(|each_elem| each_elem.is_reflectable());
    }

    // ret: 最初にマッチした Reflectable な子ノード
    pub fn find_first_child_node(&self, patterns: Vec<&str>) -> Option<&SyntaxNode> {
        for each_elem in &self.subelems {
            match each_elem {
                SyntaxNodeChild::Node(node) => {
                    match &node.ast_reflection_style {
                        ASTReflectionStyle::Reflection(name) if patterns.iter().any(|s| s == name) => return Some(node),
                        _ => (),
                    }
                },
                _ => (),
            }
        }

        return None;
    }

    // ret: すべてのマッチした Reflectable な子ノードの列
    pub fn find_child_nodes(&self, patterns: Vec<&str>) -> Vec<&SyntaxNode> {
        let mut nodes = Vec::<&SyntaxNode>::new();

        for each_elem in &self.subelems {
            match each_elem {
                SyntaxNodeChild::Node(node) => {
                    match &node.ast_reflection_style {
                        ASTReflectionStyle::Reflection(name) if patterns.iter().any(|s| s == name) => nodes.push(node),
                        _ => (),
                    }
                },
                _ => (),
            }
        }

        return nodes;
    }

    // todo: 最初に出現したリーフの位置を返す; Unreflectable なリーフも対象にする
    pub fn get_position(&self, cons: &Rc<RefCell<Console>>) -> ConsoleResult<CharacterPosition> {
        for each_child in self.get_children() {
            match each_child {
                SyntaxNodeChild::Leaf(each_leaf) => return Ok(each_leaf.pos.clone()),
                _ => (),
            }
        };

        cons.borrow_mut().append_log(TreeLog::CharacterPositionNotFound {
            uuid: self.uuid.clone(),
        }.get_log());

        return Err(());
    }

    pub fn get_children(&self) -> &Vec<SyntaxNodeChild> {
        return &self.subelems;
    }

    pub fn get_child_at(&self, cons: &Rc<RefCell<Console>>, index: usize) -> ConsoleResult<&SyntaxNodeChild> {
        let mut elem_i = 0;
        let mut reflectable_elem_i = 0;

        for each_elem in &self.subelems {
            if each_elem.is_reflectable() {
                if reflectable_elem_i == index {
                    return match self.subelems.get(elem_i) {
                        Some(v) => Ok(&v),
                        None => {
                            cons.borrow_mut().append_log(TreeLog::NodeChildNotFound {
                                parent_uuid: self.uuid.clone(),
                                index: index,
                            }.get_log());

                            return Err(());
                        },
                    };
                }

                reflectable_elem_i += 1;
            }

            elem_i += 1;
        }

        cons.borrow_mut().append_log(TreeLog::ReflectableChildNotFound {
            parent_uuid: self.uuid,
            index: index,
        }.get_log());

        return Err(());
    }

    pub fn get_node_child_at(&self, cons: &Rc<RefCell<Console>>, index: usize) -> ConsoleResult<&SyntaxNode> {
        return self.get_child_at(cons, index)?.get_node(cons);
    }

    pub fn get_leaf_child_at(&self, cons: &Rc<RefCell<Console>>, index: usize) -> ConsoleResult<&SyntaxLeaf> {
        return self.get_child_at(cons, index)?.get_leaf(cons);
    }

    pub fn is_reflectable(&self) -> bool {
        return self.ast_reflection_style.is_reflectable();
    }

    // note: Reflectable な子孫ノードの値をすべて結合して返す
    pub fn join_child_leaf_values(&self) -> String {
        let mut s = String::new();

        for each_elem in &self.subelems {
            match each_elem {
                SyntaxNodeChild::Node(node) => {
                    s += node.join_child_leaf_values().as_str();
                },
                SyntaxNodeChild::Leaf(leaf) => {
                    match leaf.ast_reflection_style {
                        ASTReflectionStyle::Reflection(_) => s += leaf.value.as_ref(),
                        _ => (),
                    }
                },
            }
        }

        return s;
    }

    pub fn print(&self, ignore_hidden_elems: bool) {
        self.print_with_details(0, &mut BufWriter::new(stdout().lock()), ignore_hidden_elems);
    }

    pub fn print_with_details(&self, nest: usize, writer: &mut BufWriter<StdoutLock>, ignore_hidden_elems: bool) {
        if ignore_hidden_elems && !self.is_reflectable() {
            return;
        }

        let display_name = match &self.ast_reflection_style {
            ASTReflectionStyle::Reflection(elem_name) => {
                if elem_name == "" {
                    "[noname]".to_string()
                } else {
                    elem_name.clone()
                }
            },
            ASTReflectionStyle::NoReflection => "[hidden]".to_string(),
            ASTReflectionStyle::Expansion => "[expandable]".to_string(),
        };

        let uuid_str = self.uuid.to_string()[..8].to_string();

        writeln!(writer, "|{} {} *{}", "   |".repeat(nest), display_name, uuid_str).unwrap();

        for each_elem in &self.subelems {
            each_elem.print_with_details(nest + 1, writer, ignore_hidden_elems);
        }
    }
}

#[derive(Clone)]
pub struct SyntaxLeaf {
    pub uuid: Uuid,
    pub pos: CharacterPosition,
    pub value: String,
    pub ast_reflection_style: ASTReflectionStyle,
}

impl SyntaxLeaf {
    pub fn new(uuid: Uuid, pos: CharacterPosition, value: String, ast_reflection_style: ASTReflectionStyle) -> SyntaxLeaf {
        return SyntaxLeaf {
            pos: pos,
            value: value,
            ast_reflection_style: ast_reflection_style,
            uuid: uuid,
        };
    }

    pub fn is_reflectable(&self) -> bool {
        return self.ast_reflection_style.is_reflectable();
    }

    pub fn print(&self, ignore_hidden_elems: bool) {
        self.print_with_details(0, &mut BufWriter::new(stdout().lock()), ignore_hidden_elems);
    }

    pub fn print_with_details(&self, nest: usize, writer: &mut BufWriter<StdoutLock>, ignore_hidden_elems: bool) {
        if !self.is_reflectable() && ignore_hidden_elems {
            return;
        }

        let value = self.value
            .replace("\\", "\\\\")
            .replace("\n", "\\n")
            .replace("\t", "\\t");

        let ast_reflection_str = match &self.ast_reflection_style {
            ASTReflectionStyle::Reflection(elem_name) => format!("({})", elem_name.clone()),
            ASTReflectionStyle::NoReflection => "[hidden]".to_string(),
            ASTReflectionStyle::Expansion => "[expandable]".to_string(),
        };

        let pos_str = format!("{}:{}", self.pos.line + 1, self.pos.column + 1);
        let uuid_str = self.uuid.to_string()[..8].to_string();

        writeln!(writer, "|{}- \"{}\" {} {} *{}", "   |".repeat(nest), value, pos_str, ast_reflection_str, uuid_str).unwrap();
    }
}
