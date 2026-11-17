//! Lecture d'une réponse XML (`format: xml`) : le document est converti en
//! JSON, et les mêmes chemins JSONPath s'y appliquent.
//!
//! La conversion, sans réglage caché :
//!
//! * le document devient `{"<racine>": …}` ;
//! * un élément sans attribut ni élément enfant devient son texte (sans les
//!   blancs qui l'entourent) : `<state>up</state>` → `"up"` ;
//! * sinon, un objet : chaque attribut sous `@<nom>`, le texte éventuel sous
//!   `#text`, chaque enfant sous son nom (sans préfixe d'espace de noms) ;
//! * plusieurs enfants du même nom deviennent une liste, et un nom cité dans
//!   `lists` est toujours une liste, même quand la réponse n'en porte qu'un :
//!   `$.response.result.entry[*]` se lit alors de la même façon avec une ou
//!   dix entrées.
//!
//! Une DTD est refusée (pas d'entités à développer), et la profondeur est
//! bornée : au-delà, le sous-arbre est ignoré.

use serde_json::{Map, Value};

/// Profondeur maximale convertie.
const MAX_DEPTH: usize = 64;

pub(crate) fn to_json(text: &str, lists: &[String]) -> Result<Value, String> {
    let document = roxmltree::Document::parse(text).map_err(|error| error.to_string())?;
    let root = document.root_element();
    let mut top = Map::new();
    top.insert(root.tag_name().name().to_string(), element(root, lists, 0));
    Ok(Value::Object(top))
}

fn own_text(node: roxmltree::Node<'_, '_>) -> String {
    let text: String =
        node.children().filter(|child| child.is_text()).filter_map(|child| child.text()).collect();
    text.trim().to_string()
}

fn element(node: roxmltree::Node<'_, '_>, lists: &[String], depth: usize) -> Value {
    let has_children = node.children().any(|child| child.is_element());
    if !has_children && node.attributes().next().is_none() {
        return Value::String(own_text(node));
    }
    let mut map = Map::new();
    for attribute in node.attributes() {
        map.insert(format!("@{}", attribute.name()), Value::String(attribute.value().to_string()));
    }
    let text = own_text(node);
    if !text.is_empty() {
        map.insert("#text".to_string(), Value::String(text));
    }
    if depth >= MAX_DEPTH {
        return Value::Object(map);
    }
    for child in node.children().filter(|child| child.is_element()) {
        let name = child.tag_name().name();
        let value = element(child, lists, depth + 1);
        // Une valeur d'élément n'est jamais une liste : une liste trouvée ici
        // est celle des homonymes déjà vus.
        match map.get_mut(name) {
            Some(Value::Array(items)) => items.push(value),
            Some(existing) => {
                let first = existing.take();
                *existing = Value::Array(vec![first, value]);
            }
            None if lists.iter().any(|list| list == name) => {
                map.insert(name.to_string(), Value::Array(vec![value]));
            }
            None => {
                map.insert(name.to_string(), value);
            }
        }
    }
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn un_document_devient_des_objets_des_textes_et_des_listes() {
        let xml = r#"<response status="success"><result>
            <hostname> fw1 </hostname>
            <entry><name>ethernet1/1</name></entry>
            <entry><name>ethernet1/2</name></entry>
            <empty/>
        </result></response>"#;
        assert_eq!(
            to_json(xml, &[]).unwrap(),
            json!({"response": {"@status": "success", "result": {
                "hostname": "fw1",
                "entry": [{"name": "ethernet1/1"}, {"name": "ethernet1/2"}],
                "empty": ""
            }}})
        );
    }

    #[test]
    fn un_element_cite_dans_lists_est_toujours_une_liste() {
        let xml = "<r><entry><name>a</name></entry><one><name>b</name></one></r>";
        assert_eq!(
            to_json(xml, &["entry".to_string()]).unwrap(),
            json!({"r": {"entry": [{"name": "a"}], "one": {"name": "b"}}})
        );
    }

    #[test]
    fn texte_et_attributs_se_cotoient() {
        let xml = r#"<r><v unit="C">42<x>1</x></v></r>"#;
        assert_eq!(
            to_json(xml, &[]).unwrap(),
            json!({"r": {"v": {"@unit": "C", "#text": "42", "x": "1"}}})
        );
    }

    #[test]
    fn une_dtd_ou_un_document_mal_forme_est_refuse() {
        assert!(to_json("<!DOCTYPE r [<!ENTITY a \"b\">]><r>&a;</r>", &[]).is_err());
        assert!(to_json("<r><unclosed></r>", &[]).is_err());
        assert!(to_json("not xml", &[]).is_err());
    }
}
