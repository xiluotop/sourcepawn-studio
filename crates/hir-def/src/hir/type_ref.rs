use std::fmt;

use syntax::TSKind;
use tree_sitter::Node;

use crate::item_tree::Name;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum TypeRef {
    /// Reference to a type definition (e.g. enum struct, enum, methodmap, etc.)
    Name(Name),

    /// Old name
    OldName(Name),

    /// int
    Int,

    /// int64
    Int64,

    /// bool
    Bool,

    /// float
    Float,

    /// char
    Char,

    /// void
    Void,

    /// any
    Any,

    /// String
    OldString,

    /// Float
    OldFloat,

    /// Array
    Array((Box<TypeRef>, usize)),
}

impl TypeRef {
    pub fn from_node(node: &Node, source: &str) -> Self {
        // tree-sitter-sourcepawn does not expose `int64` as a builtin type yet,
        // so it is currently parsed as an identifier. Keep this text check even
        // after the grammar catches up so projects can update the grammar
        // independently without changing the HIR representation.
        if node.utf8_text(source.as_bytes()).unwrap_or_default() == "int64" {
            return Self::Int64;
        }

        match TSKind::from(node) {
            TSKind::anon_int => Self::Int,
            TSKind::anon_bool => Self::Bool,
            TSKind::anon_float => Self::Float,
            TSKind::anon_char => Self::Char,
            TSKind::anon_void => Self::Void,
            TSKind::any_type => Self::Any,
            TSKind::anon_String => Self::OldString,
            TSKind::anon_Float => Self::Float,
            TSKind::r#type => TypeRef::Name(Name::from_node(node, source)),
            TSKind::old_type => {
                let text = node
                    .utf8_text(source.as_bytes())
                    .expect("Failed to get utf8 text")
                    .trim_end_matches(':');
                TypeRef::OldName(Name::from(text))
            }
            _ => TypeRef::Name(Name::from_node(node, source)),
        }
    }

    pub fn from_returntype_node(node: &Node, field_name: &str, source: &str) -> Option<Self> {
        let mut type_ref = None;
        let mut size = 0;
        for child in node.children_by_field_name(field_name, &mut node.walk()) {
            match TSKind::from(child) {
                TSKind::dimension | TSKind::fixed_dimension => {
                    size += 1;
                }
                _ => {
                    type_ref = Some(TypeRef::from_node(&child, source));
                }
            }
        }
        if let Some(type_ref) = type_ref {
            if size > 0 {
                Some(Self::Array((Box::new(type_ref), size)))
            } else {
                Some(type_ref)
            }
        } else {
            None
        }
    }

    pub fn to_lower_dim(&self) -> Self {
        match self {
            TypeRef::Array((type_ref, size)) => {
                if *size > 1 {
                    TypeRef::Array((type_ref.clone(), size - 1))
                } else {
                    self.clone()
                }
            }
            _ => self.clone(),
        }
    }
}

impl fmt::Display for TypeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            TypeRef::Name(name) => String::from(name.clone()), //TODO: Can we avoid this clone?
            TypeRef::OldName(name) => format!("{}:", name),
            TypeRef::Int => "int".to_string(),
            TypeRef::Int64 => "int64".to_string(),
            TypeRef::Bool => "bool".to_string(),
            TypeRef::Float => "float".to_string(),
            TypeRef::Char => "char".to_string(),
            TypeRef::Void => "void".to_string(),
            TypeRef::Any => "any".to_string(),
            TypeRef::OldString => "String".to_string(),
            TypeRef::OldFloat => "Float".to_string(),
            TypeRef::Array((type_ref, size)) => {
                let mut res = type_ref.to_string();
                res.push_str(&"[]".repeat(*size));
                res
            }
        };

        write!(f, "{}", s)
    }
}

impl TypeRef {
    /// Returns the type as a string without the array brackets or
    /// the colon for old types
    pub fn type_as_string(&self) -> String {
        match self {
            TypeRef::Name(name) => name.to_string(),
            TypeRef::OldName(name) => name.to_string(),
            TypeRef::Int => "int".to_string(),
            TypeRef::Int64 => "int64".to_string(),
            TypeRef::Bool => "bool".to_string(),
            TypeRef::Float => "float".to_string(),
            TypeRef::Char => "char".to_string(),
            TypeRef::Void => "void".to_string(),
            TypeRef::Any => "any".to_string(),
            TypeRef::OldString => "String".to_string(),
            TypeRef::OldFloat => "Float".to_string(),
            TypeRef::Array((type_ref, _)) => type_ref.to_string(),
        }
    }
}

pub fn type_string_from_node(node: &Node, source: &str) -> String {
    TypeRef::from_node(node, source).type_as_string()
}

#[cfg(test)]
mod tests {
    use super::TypeRef;

    fn parse_first_item(source: &str) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_sourcepawn::language())
            .expect("Failed to set SourcePawn language");
        parser.parse(source, None).expect("Failed to parse source")
    }

    #[test]
    fn recognizes_int64_variable_type() {
        let source = "int64 value = 2147483648;";
        let tree = parse_first_item(source);
        let item = tree.root_node().named_child(0).expect("Missing item");

        assert!(!item.has_error());
        assert_eq!(
            TypeRef::from_returntype_node(&item, "type", source),
            Some(TypeRef::Int64)
        );
    }

    #[test]
    fn recognizes_int64_function_return_type() {
        let source = "int64 get_value() { return 2147483648; }";
        let tree = parse_first_item(source);
        let item = tree.root_node().named_child(0).expect("Missing item");

        assert!(!item.has_error());
        assert_eq!(
            TypeRef::from_returntype_node(&item, "returnType", source),
            Some(TypeRef::Int64)
        );
    }
}
