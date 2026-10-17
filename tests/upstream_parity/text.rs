//! Mirrors `TextTests`, `MTextTests`, `AttributeEntityTests` and
//! `AttributeDefinitionTests`: the string (including non-ASCII), the position, the
//! height and the rotation, and an attribute on an insert becoming visible text.
//!
//! Wire 2 carries a text's position, height, rotation and string and nothing
//! else: no horizontal or vertical alignment, no style name, no width factor
//! (acadsharp-rs-tests#66). The upstream suites assert some of those; they are not
//! asserted here because the safe API has no field to read them from.

use acadsharp_rs::Primitive;

use crate::common::*;

fn texts() -> Vec<acadsharp_rs::item::Text> {
    let o = open("parity_text");
    primitives(&o.document, 0)
        .into_iter()
        .filter_map(|p| {
            if let Primitive::Text(t) = p {
                Some(t)
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn a_text_keeps_its_non_ascii_string_position_height_and_rotation() {
    let t = texts()
        .into_iter()
        .find(|t| t.text == "Grüße 北京 – ünï")
        .expect("the non-ASCII TEXT, byte for byte");
    assert_near3("position", t.position, [10.0, 20.0, 0.0]);
    assert_near("height", t.height, 2.5);
    assert_near("rotation", t.rotation, 0.3);
}

#[test]
fn an_mtext_keeps_its_content_position_and_height() {
    let t = texts()
        .into_iter()
        .find(|t| t.text == "MTEXT content")
        .expect("the MTEXT");
    assert_near3("position", t.position, [30.0, 40.0, 0.0]);
    assert_near("height", t.height, 1.8);
}

#[test]
fn an_attribute_definition_at_the_top_of_a_drawing_is_ordinary_text() {
    let t = texts()
        .into_iter()
        .find(|t| t.text == "ATTDEF-VALUE")
        .expect("the ATTDEF, as the text its default value is");
    assert_near3("position", t.position, [50.0, 0.0, 0.0]);
    assert_near("height", t.height, 1.2);
}

#[test]
fn an_attribute_on_an_insert_becomes_visible_text_where_the_generator_put_it() {
    let t = texts()
        .into_iter()
        .find(|t| t.text == "ATT-VISIBLE")
        .expect("the insert's attribute as a Text");
    assert_near3("position", t.position, [60.0, 2.0, 0.0]);
    assert_near("height", t.height, 1.5);
}

#[test]
fn no_text_is_invented_or_lost() {
    // Four strings written at the top level or on the insert, so four texts.
    let all: Vec<String> = texts().into_iter().map(|t| t.text).collect();
    assert_eq!(all.len(), 4, "{all:?}");
}
