use window::ContextMenuItem;

pub fn flatten_menu(items: &[ContextMenuItem]) -> Vec<(String, usize)> {
    let mut result = Vec::new();
    for item in items {
        match item {
            ContextMenuItem::Entry { label, tag } => result.push((label.clone(), *tag)),
            ContextMenuItem::Separator => {}
            ContextMenuItem::SubMenu { label, items } => {
                result.extend(
                    flatten_menu(items)
                        .into_iter()
                        .map(|(child, tag)| (format!("{label} / {child}"), tag)),
                );
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_actions_and_submenu_labels_without_separators() {
        let items = vec![
            ContextMenuItem::Entry {
                label: "Rename".into(),
                tag: 1,
            },
            ContextMenuItem::Separator,
            ContextMenuItem::SubMenu {
                label: "Color".into(),
                items: vec![
                    ContextMenuItem::Entry {
                        label: "Blue".into(),
                        tag: 106,
                    },
                    ContextMenuItem::SubMenu {
                        label: "More".into(),
                        items: vec![ContextMenuItem::Entry {
                            label: "Reset".into(),
                            tag: 100,
                        }],
                    },
                ],
            },
            ContextMenuItem::Entry {
                label: "Close".into(),
                tag: 6,
            },
        ];
        assert_eq!(
            flatten_menu(&items),
            vec![
                ("Rename".into(), 1),
                ("Color / Blue".into(), 106),
                ("Color / More / Reset".into(), 100),
                ("Close".into(), 6),
            ]
        );
    }
}
