use std::collections::*;

use crate::block::*;
use crate::data::*;
use crate::rule::*;

use regex::*;

use rustnutlib::*;
use rustnutlib::console::*;

pub type SyntaxParseResult<T> = Result<T, SyntaxParseError>;

pub enum SyntaxParseError {
    Unknown(),
    BlockParseErr(BlockParseError),
    InternalErr(String),
    InvalidCharClassFormat(String),
    InvalidMacroArgumentLength(Vec<String>),
    InvalidSyntaxTreeStructure(String),
    NoSucceededRule(String, usize, Vec<(usize, String)>),
    TooDeepRecursion(usize),
    TooLongRepeat(usize),
    UnknownMacroArgID(String),
    UnknownRuleID(String),
}

impl ConsoleLogger for SyntaxParseError {
    fn get_log(&self) -> ConsoleLog {
        return match self {
            SyntaxParseError::Unknown() => log!(Error, "unknown error"),
            SyntaxParseError::BlockParseErr(err) => err.get_log(),
            SyntaxParseError::InternalErr(err_msg) => log!(Error, &format!("internal error: {}", err_msg)),
            SyntaxParseError::InvalidCharClassFormat(value) => log!(Error, &format!("invalid character class format '{}'", value)),
            SyntaxParseError::InvalidMacroArgumentLength(arg_names) => log!(Error, &format!("invalid macro argument length ({:?})", arg_names)),
            SyntaxParseError::InvalidSyntaxTreeStructure(cause) => log!(Error, &format!("invalid syntax tree structure ({})", cause)),
            SyntaxParseError::NoSucceededRule(rule_id, src_i, rule_stack) => log!(Error, &format!("no succeeded rule '{}' at {} in the source", rule_id, src_i + 1), format!("rule stack: {:?}", rule_stack)),
            SyntaxParseError::TooDeepRecursion(max_recur_count) => log!(Error, &format!("too deep recursion over {}", max_recur_count)),
            SyntaxParseError::TooLongRepeat(max_loop_count) => log!(Error, &format!("too long repeat over {}", max_loop_count)),
            SyntaxParseError::UnknownMacroArgID(macro_arg_id) => log!(Error, &format!("unknown macro arg id '{}'", macro_arg_id)),
            SyntaxParseError::UnknownRuleID(rule_id) => log!(Error, &format!("unknown rule id '{}'", rule_id)),
        };
    }
}

pub struct SyntaxParser {
    rule_map: RuleMap,
    src_i: usize,
    src_line: usize,
    src_latest_line_i: usize,
    src_content: String,
    recursion_count: usize,
    max_recursion_count: usize,
    max_loop_count: usize,
    rule_stack: Vec<(usize, String)>,
    regex_map: HashMap::<String, Regex>,
}

impl SyntaxParser {
    pub fn new(rule_map: RuleMap) -> SyntaxParseResult<SyntaxParser> {
        return Ok(SyntaxParser {
            rule_map: rule_map,
            src_i: 0,
            src_line: 0,
            src_latest_line_i: 0,
            src_content: String::new(),
            recursion_count: 1,
            max_recursion_count: 65536,
            max_loop_count: 65536,
            rule_stack: vec![],
            regex_map: HashMap::new(),
        });
    }

    pub fn get_syntax_tree(&mut self, src_content: &String) -> SyntaxParseResult<SyntaxTree> {
        let mut tmp_src_content = src_content.clone();

        // todo: 高速化: replace() と比べてどちらが速いか検証する
        // note: 余分な改行コード 0x0d を排除する
        loop {
            match tmp_src_content.find(0x0d as char) {
                Some(v) => {
                    tmp_src_content.remove(v);
                },
                None => break,
            }
        }

        // EOF 用のヌル文字
        tmp_src_content += "\0";

        // フィールドを初期化
        self.src_i = 0;
        self.src_content = tmp_src_content;
        self.recursion_count = 1;

        let start_rule_id = self.rule_map.start_rule_id.clone();

        if self.src_content.chars().count() == 0 {
            return Ok(SyntaxTree::from_node_args(vec![], ASTReflectionStyle::from_config(false, String::new())));
        }

        self.recursion_count += 1;

        let mut root_node = match self.is_rule_successful(&HashMap::new(), &start_rule_id)? {
            Some(v) => v,
            None => return Err(SyntaxParseError::NoSucceededRule(start_rule_id.clone(), self.src_i, self.rule_stack.clone())),
        };

        // ルートは常に Reflectable
        root_node.set_ast_reflection(ASTReflectionStyle::Reflection(start_rule_id.clone()));

        if self.src_i < self.src_content.chars().count() {
            return Err(SyntaxParseError::NoSucceededRule(start_rule_id.clone(), self.src_i, self.rule_stack.clone()));
        }

        self.recursion_count -= 1;
        return Ok(SyntaxTree::from_node(root_node));
    }

    fn is_rule_successful(&mut self, macro_def_args: &HashMap<String, Box<RuleGroup>>, rule_id: &String) -> SyntaxParseResult<Option<SyntaxNodeElement>> {
        let rule = match self.rule_map.get_rule(rule_id) {
            Some(v) => v.clone(),
            None => return Err(SyntaxParseError::UnknownRuleID(rule_id.clone())),
        };

        self.rule_stack.push((self.src_i, rule_id.clone()));

        return match self.is_choice_successful(macro_def_args, &rule.group.elem_order, &rule.group)? {
            Some(v) => {
                let mut ast_reflection_style = match &rule.group.sub_elems.get(0) {
                    Some(v) => {
                        match v {
                            RuleElement::Group(sub_choice) => sub_choice.ast_reflection_style.clone(),
                            RuleElement::Expression(_) => rule.group.ast_reflection_style.clone(),
                        }
                    },
                    _ => rule.group.ast_reflection_style.clone(),
                };

                match &ast_reflection_style {
                    ASTReflectionStyle::Reflection(elem_name) => {
                        if *elem_name == String::new() {
                            ast_reflection_style = ASTReflectionStyle::from_config(true, rule_id.clone())
                        }
                    },
                    _ => (),
                };

                self.rule_stack.pop().unwrap();
                let new_node = SyntaxNodeElement::from_node_args(v, ast_reflection_style);
                Ok(Some(new_node))
            },
            None => Ok(None),
        }
    }

    fn is_choice_successful(&mut self, macro_def_args: &HashMap<String, Box<RuleGroup>>, parent_elem_order: &RuleElementOrder, group: &Box<RuleGroup>) -> SyntaxParseResult<Option<Vec<SyntaxNodeElement>>> {
        return self.is_lookahead_choice_successful(macro_def_args, parent_elem_order, group);
    }

    fn is_lookahead_choice_successful(&mut self, macro_def_args: &HashMap<String, Box<RuleGroup>>, parent_elem_order: &RuleElementOrder, group: &Box<RuleGroup>) -> SyntaxParseResult<Option<Vec<SyntaxNodeElement>>> {
        return if group.lookahead_kind.is_none() {
            self.is_loop_choice_successful(macro_def_args, parent_elem_order, group)
        } else {
            let start_src_i = self.src_i;
            let is_lookahead_positive = group.lookahead_kind == RuleElementLookaheadKind::Positive;

            let is_choice_successful = self.is_loop_choice_successful(macro_def_args, parent_elem_order, group)?;
            self.src_i = start_src_i;

            if is_choice_successful.is_some() == is_lookahead_positive {
                Ok(Some(vec![]))
            } else {
                Ok(None)
            }
        }
    }

    fn is_loop_choice_successful(&mut self, macro_def_args: &HashMap<String, Box<RuleGroup>>, parent_elem_order: &RuleElementOrder, group: &Box<RuleGroup>) -> SyntaxParseResult<Option<Vec<SyntaxNodeElement>>> {
        let (min_count, max_count) = match parent_elem_order {
            RuleElementOrder::Random(tmp_occurrence_count) => {
                let (mut tmp_min_count, mut tmp_max_count) = tmp_occurrence_count.to_tuple();

                // todo: 0 だった場合大丈夫かを確認
                tmp_min_count += group.loop_count.min - 1;

                if tmp_max_count != -1 {
                    let max_num = match group.loop_count.max {
                        Infinitable::Normal(v) => v as i32,
                        Infinitable::Infinite => -1,
                    };

                    tmp_max_count += max_num - 1;
                }

                (tmp_min_count, tmp_max_count)
            },
            RuleElementOrder::Sequential => group.loop_count.to_tuple(),
        };

        if max_count != -1 && min_count as i32 > max_count {
            return Err(SyntaxParseError::InternalErr(format!("invalid loop count {{{},{}}}", min_count, max_count)));
        }

        let mut children = Vec::<SyntaxNodeElement>::new();
        let mut loop_count = 0i32;

        while self.src_i < self.src_content.chars().count() {
            if loop_count > self.max_loop_count as i32 {
                return Err(SyntaxParseError::TooLongRepeat(self.max_loop_count as usize));
            }

            match self.is_each_choice_matched(macro_def_args, group)? {
                Some(node_elems) => {
                    for each_elem in node_elems {
                        match &each_elem {
                            SyntaxNodeElement::Node(node) => {
                                if node.sub_elems.len() != 0 {
                                    children.push(each_elem);
                                }
                            },
                            _ => children.push(each_elem),
                        }
                    }

                    loop_count += 1;

                    if max_count != -1 && loop_count == max_count {
                        return Ok(Some(children));
                    }
                },
                None => {
                    if loop_count >= min_count as i32 && (max_count == -1 || loop_count <= max_count) {
                        return Ok(Some(children));
                    } else {
                        return Ok(None);
                    }
                },
            }
        }

        if loop_count >= min_count as i32 && (max_count == -1 || loop_count <= max_count) {
            return Ok(Some(children));
        } else {
            return Ok(None);
        }
    }

    fn is_each_choice_matched(&mut self, macro_def_args: &HashMap<String, Box<RuleGroup>>, group: &Box<RuleGroup>) -> SyntaxParseResult<Option<Vec<SyntaxNodeElement>>> {
        let mut children = Vec::<SyntaxNodeElement>::new();

        for each_elem in &group.sub_elems {
            let start_src_i = self.src_i;

            match each_elem {
                RuleElement::Group(each_group) => {
                    match &each_group.elem_order {
                        RuleElementOrder::Random(_) => {
                            let mut new_sub_children = Vec::<SyntaxNodeElement>::new();
                            let mut matched_choices = [false].repeat(each_group.sub_elems.len());

                            for _i in 0..each_group.sub_elems.len() {
                                for (sub_elem_i, each_sub_elem) in each_group.sub_elems.iter().enumerate() {
                                    let is_check_done = *matched_choices.get(sub_elem_i).unwrap();
                                    let elem_start_src_i = self.src_i;

                                    match each_sub_elem {
                                        RuleElement::Group(each_sub_choice) if !is_check_done => {
                                            match self.is_choice_successful(macro_def_args, &each_group.elem_order, each_sub_choice)? {
                                                Some(v) => {
                                                    for each_result_sub_elem in v {
                                                        match each_result_sub_elem {
                                                            SyntaxNodeElement::Node(node) if node.sub_elems.len() == 0 => (),
                                                            _ => {
                                                                match each_result_sub_elem {
                                                                    SyntaxNodeElement::Node(result_node) if result_node.ast_reflection_style.is_expandable() => {
                                                                        new_sub_children.append(&mut result_node.sub_elems.clone());
                                                                    },
                                                                    _ => new_sub_children.push(each_result_sub_elem),
                                                                }
                                                            },
                                                        }
                                                    }

                                                    matched_choices[sub_elem_i] = true;
                                                    break;
                                                },
                                                None => {
                                                    self.src_i = elem_start_src_i;
                                                    continue;
                                                },
                                            }
                                        },
                                        _ => (),
                                    }
                                }
                            }

                            if matched_choices.contains(&false) {
                                return Ok(None);
                            }

                            let new_child = SyntaxNodeElement::from_node_args(new_sub_children, each_group.ast_reflection_style.clone());

                            match new_child {
                                SyntaxNodeElement::Node(node) if node.sub_elems.len() == 0 => (),
                                _ => children.push(new_child),
                            }
                        },
                        RuleElementOrder::Sequential => {
                            match each_group.kind {
                                RuleGroupKind::Choice => {
                                    let mut is_successful = false;

                                    for each_sub_elem in &each_group.sub_elems {
                                        match each_sub_elem {
                                            RuleElement::Group(each_sub_group) => {
                                                match self.is_choice_successful(macro_def_args, &each_group.elem_order, each_sub_group)? {
                                                    Some(v) => {
                                                        if group.sub_elems.len() != 1 {
                                                            let new_child = SyntaxNodeElement::from_node_args(v, each_sub_group.ast_reflection_style.clone());

                                                            match new_child {
                                                                SyntaxNodeElement::Node(node) if node.sub_elems.len() == 0 => (),
                                                                _ => {
                                                                    match new_child {
                                                                        SyntaxNodeElement::Node(new_node) if new_node.ast_reflection_style.is_expandable() => {
                                                                            children.append(&mut new_node.sub_elems.clone());
                                                                        },
                                                                        _ => children.push(new_child),
                                                                    }
                                                                },
                                                            }
                                                        } else {
                                                            children = v;
                                                        }

                                                        is_successful = true;
                                                        break;
                                                    },
                                                    None => {
                                                        self.src_i = start_src_i;
                                                    },
                                                }
                                            },
                                            _ => (),
                                        }
                                    }

                                    if !is_successful {
                                        return Ok(None);
                                    }
                                },
                                RuleGroupKind::Sequence => {
                                    match self.is_choice_successful(macro_def_args, &each_group.elem_order, each_group)? {
                                        Some(v) => {
                                            if group.sub_elems.len() != 1 {
                                                let new_child = SyntaxNodeElement::from_node_args(v, each_group.ast_reflection_style.clone());

                                                match new_child {
                                                    SyntaxNodeElement::Node(node) if node.sub_elems.len() == 0 => (),
                                                    _ => {
                                                        match new_child {
                                                            SyntaxNodeElement::Node(new_node) if new_node.ast_reflection_style.is_expandable() => {
                                                                children.append(&mut new_node.sub_elems.clone());
                                                            },
                                                            _ => children.push(new_child),
                                                        }
                                                    },
                                                }
                                            } else {
                                                children = v;
                                            }

                                            continue;
                                        },
                                        None => {
                                            self.src_i = start_src_i;
                                            return Ok(None);
                                        },
                                    }
                                },
                            }
                        },
                    }
                },
                RuleElement::Expression(each_expr) => {
                    match self.is_expr_successful(macro_def_args, each_expr)? {
                        Some(node_elems) => {
                            for each_elem in node_elems {
                                match each_elem {
                                    SyntaxNodeElement::Node(node) if node.sub_elems.len() == 0 => (),
                                    _ => children.push(each_elem),
                                }
                            }

                            continue;
                        },
                        None => {
                            self.src_i = start_src_i;
                            return Ok(None);
                        },
                    }
                }
            }
        }

        return Ok(Some(children));
    }

    fn is_expr_successful(&mut self, macro_def_args: &HashMap<String, Box<RuleGroup>>, expr: &Box<RuleExpression>) -> SyntaxParseResult<Option<Vec<SyntaxNodeElement>>> {
        return self.is_lookahead_expr_successful(macro_def_args, expr);
    }

    fn is_lookahead_expr_successful(&mut self, macro_def_args: &HashMap<String, Box<RuleGroup>>, expr: &Box<RuleExpression>) -> SyntaxParseResult<Option<Vec<SyntaxNodeElement>>> {
        return if expr.lookahead_kind.is_none() {
            self.is_loop_expr_successful(macro_def_args, expr)
        } else {
            let start_src_i = self.src_i;
            let is_lookahead_positive = expr.lookahead_kind == RuleElementLookaheadKind::Positive;

            let is_expr_successful = self.is_loop_expr_successful(macro_def_args, expr)?;
            self.src_i = start_src_i;

            if is_expr_successful.is_some() == is_lookahead_positive {
                Ok(Some(vec![]))
            } else {
                Ok(None)
            }
        }
    }

    fn is_loop_expr_successful(&mut self, macro_def_args: &HashMap<String, Box<RuleGroup>>, expr: &Box<RuleExpression>) -> SyntaxParseResult<Option<Vec<SyntaxNodeElement>>> {
        let (min_count, max_count) = expr.loop_count.to_tuple();

        if max_count != -1 && min_count as i32 > max_count {
            return Err(SyntaxParseError::InternalErr(format!("invalid loop count {{{},{}}}", min_count, max_count)));
        }

        let mut children = Vec::<SyntaxNodeElement>::new();
        let mut loop_count = 0usize;

        while self.src_i < self.src_content.chars().count() {
            if loop_count > self.max_loop_count {
                return Err(SyntaxParseError::TooLongRepeat(self.max_loop_count as usize));
            }

            match self.is_each_expr_matched(macro_def_args, expr)? {
                Some(node) => {
                    for each_node in node {
                        match each_node {
                            SyntaxNodeElement::Node(node) if node.sub_elems.len() == 0 => (),
                            _ => children.push(each_node),
                        }
                    }

                    loop_count += 1;

                    if max_count != -1 && loop_count as i32 == max_count {
                        return Ok(Some(children));
                    }
                },
                None => {
                    return if loop_count >= min_count && (max_count == -1 || loop_count as i32 <= max_count) {
                        Ok(Some(children))
                    } else {
                        Ok(None)
                    }
                },
            }
        }

        return if loop_count >= min_count && (max_count == -1 || loop_count as i32 <= max_count) {
            Ok(Some(children))
        } else {
            Ok(None)
        }
    }

    fn is_each_expr_matched(&mut self, macro_def_args: &HashMap<String, Box<RuleGroup>>, expr: &Box<RuleExpression>) -> SyntaxParseResult<Option<Vec<SyntaxNodeElement>>> {
        if self.src_i >= self.src_content.chars().count() {
            return Ok(None);
        }

        match &expr.kind {
            RuleExpressionKind::CharClass => {
                if self.src_content.chars().count() < self.src_i + 1 {
                    return Ok(None);
                }

                // note: パターンが見つからない場合は新しく追加する
                let pattern = match self.regex_map.get(&expr.value) {
                    Some(v) => v,
                    None => {
                        let pattern = match Regex::new(&expr.value.clone()) {
                            Ok(v) => v,
                            Err(_) => return Err(SyntaxParseError::InvalidCharClassFormat(expr.to_string())),
                        };

                        self.regex_map.insert(expr.value.clone(), pattern);
                        self.regex_map.get(&expr.value).unwrap()
                    },
                };

                let tar_char = self.substring_src_content(self.src_i, 1);

                if pattern.is_match(&tar_char) {
                    let new_leaf = SyntaxNodeElement::from_leaf_args(self.get_char_position(), tar_char.clone(), expr.ast_reflection_style.clone());
                    self.add_source_index_by_string(&tar_char);

                    return Ok(Some(vec![new_leaf]));
                } else {
                    return Ok(None);
                }
            },
            RuleExpressionKind::ID => {
                self.recursion_count += 1;

                if self.max_recursion_count < self.recursion_count {
                    return Err(SyntaxParseError::TooDeepRecursion(self.max_recursion_count));
                }

                return self.is_rule_expr_matched(macro_def_args, expr);
            },
            RuleExpressionKind::MacroArgID => {
                return match macro_def_args.get(&expr.value) {
                    Some(v) => self.is_choice_successful(macro_def_args, &RuleElementOrder::Sequential, v),
                    None => Err(SyntaxParseError::UnknownMacroArgID(expr.value.clone())),
                };
            },
            RuleExpressionKind::MacroCall(arg_groups) => {
                let rule_id = &expr.value;

                let mut macro_args = HashMap::<String, Box::<RuleGroup>>::new();
                let new_macro_def_args = match self.rule_map.get_rule(rule_id) {
                    Some(rule) => &rule.macro_args,
                    None => return Err(SyntaxParseError::UnknownRuleID(rule_id.clone())),
                };

                if arg_groups.len() != new_macro_def_args.len() {
                    return Err(SyntaxParseError::InvalidMacroArgumentLength(new_macro_def_args.clone()));
                }

                for i in 0..arg_groups.len() {
                    let new_macro_id = match new_macro_def_args.get(i) {
                        Some(v) => v,
                        None => return Err(SyntaxParseError::InternalErr("invalid operation".to_string())),
                    };

                    let new_macro_group = arg_groups.get(i).unwrap();
                    macro_args.insert(new_macro_id.clone(), new_macro_group.clone());
                }

                return self.is_rule_expr_matched(&macro_args, expr);
            },
            RuleExpressionKind::String => {
                if self.src_content.chars().count() < self.src_i + expr.value.chars().count() {
                    return Ok(None);
                }

                if self.substring_src_content(self.src_i, expr.value.chars().count()) == expr.value {
                    let new_leaf = SyntaxNodeElement::from_leaf_args(self.get_char_position(), expr.value.clone(), expr.ast_reflection_style.clone());
                    self.add_source_index_by_string(&expr.value);

                    return Ok(Some(vec![new_leaf]));
                } else {
                    return Ok(None);
                }
            },
            RuleExpressionKind::Wildcard => {
                if self.src_content.chars().count() < self.src_i + 1 {
                    return Ok(None);
                }

                let expr_value = self.substring_src_content(self.src_i, 1);
                let new_leaf = SyntaxNodeElement::from_leaf_args(self.get_char_position(), expr_value.clone(), expr.ast_reflection_style.clone());
                self.add_source_index_by_string(&expr_value);

                return Ok(Some(vec![new_leaf]));
            },
        }
    }

    fn is_rule_expr_matched(&mut self, macro_def_args: &HashMap<String, Box<RuleGroup>>, expr: &Box<RuleExpression>) -> SyntaxParseResult<Option<Vec<SyntaxNodeElement>>> {
        match self.is_rule_successful(macro_def_args, &expr.value)? {
            Some(node_elem) => {
                self.recursion_count += 1;

                let conv_node_elems = match &node_elem {
                    SyntaxNodeElement::Node(node) => {
                        let sub_ast_reflection = match &expr.ast_reflection_style {
                            ASTReflectionStyle::Reflection(elem_name) => {
                                let conv_elem_name = if elem_name == "" {
                                    expr.value.clone()
                                } else {
                                    elem_name.clone()
                                };

                                ASTReflectionStyle::Reflection(conv_elem_name)
                            },
                            _ => expr.ast_reflection_style.clone(),
                        };

                        let node = SyntaxNodeElement::from_node_args(node.sub_elems.clone(), sub_ast_reflection);

                        if expr.ast_reflection_style.is_expandable() {
                            match node {
                                SyntaxNodeElement::Node(node) => node.sub_elems,
                                _ => vec![node],
                            }
                        } else {
                            vec![node]
                        }
                    },
                    SyntaxNodeElement::Leaf(_) => vec![node_elem],
                };

                return Ok(Some(conv_node_elems));
            },
            None => {
                self.recursion_count -= 1;
                return Ok(None);
            },
        };
    }

    fn substring_src_content(&self, start_i: usize, len: usize) -> String {
        return self.src_content.chars().skip(start_i).take(len).collect::<String>();
    }

    // todo: 文字列と進めるインデックスのペアとしてキャッシュを取る
    fn add_source_index_by_string(&mut self, expr_str: &String) {
        let mut new_line_indexes = Vec::<usize>::new();
        let mut char_i = 0usize;

        for each_char in expr_str.chars().rev() {
            if each_char == '\n' {
                new_line_indexes.push(char_i);

                if new_line_indexes.len() >= 2 {
                    break;
                }
            }

            char_i += 1;
        }

        match new_line_indexes.pop() {
            Some(latest_new_line_i) => {
                self.src_line += expr_str.match_indices("\n").count();
                self.src_latest_line_i = match new_line_indexes.last() {
                    Some(second_latest_new_line_i) => self.src_i + latest_new_line_i - second_latest_new_line_i + 1,
                    None => self.src_i + latest_new_line_i + 1,
                };
            },
            None => (),
        }

        self.src_i += expr_str.len();
    }

    fn get_char_position(&self) -> CharacterPosition {
        return CharacterPosition::new(self.src_i, self.src_line, self.src_i - self.src_latest_line_i);
    }
}
