use super::RealmPackage;
use serde::{Deserialize, Deserializer, Serialize, de};
use std::collections::BTreeMap;

/// A complete, atomic selection. Serialized directly into the Profile or management envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealmPackageSelection {
    pub primary_package: String,
    pub front_package: String,
    pub packages: BTreeMap<String, RealmPackage>,
}

impl RealmPackageSelection {
    /// Serde's flattened Option silently treats partial objects as absent. Only zero fields is absent.
    pub fn deserialize_optional<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Self>, D::Error> {
        fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
            d: D,
        ) -> Result<Option<T>, D::Error> {
            T::deserialize(d).map(Some)
        }
        fn package_map<'de, D: Deserializer<'de>>(
            d: D,
        ) -> Result<Option<BTreeMap<String, RealmPackage>>, D::Error> {
            let value = crate::signed_json::deserialize(d)?;
            serde_json::from_value(value)
                .map(Some)
                .map_err(de::Error::custom)
        }
        #[derive(Deserialize)]
        struct Fields {
            #[serde(default, deserialize_with = "present")]
            primary_package: Option<String>,
            #[serde(default, deserialize_with = "present")]
            front_package: Option<String>,
            #[serde(default, deserialize_with = "package_map")]
            packages: Option<BTreeMap<String, RealmPackage>>,
        }
        let fields = Fields::deserialize(d)?;
        match (
            fields.primary_package,
            fields.front_package,
            fields.packages,
        ) {
            (None, None, None) => Ok(None),
            (Some(primary_package), Some(front_package), Some(packages)) => {
                let selection = Self {
                    primary_package,
                    front_package,
                    packages,
                };
                selection.check().map_err(de::Error::custom)?;
                Ok(Some(selection))
            }
            _ => Err(de::Error::custom(
                "primary_package, front_package and packages must be supplied together",
            )),
        }
    }

    /// Resolve the Service and Front in that order.
    pub fn selected(&self) -> Result<(&RealmPackage, &RealmPackage), String> {
        let lookup = |name: &str| {
            self.packages
                .get(name)
                .ok_or_else(|| format!("unknown package reference {name:?}"))
        };
        let primary = lookup(&self.primary_package)?;
        let front = lookup(&self.front_package)?;
        if primary.r#type == front.r#type {
            return Err(
                "Service and Front must select distinct entries with different Actor types".into(),
            );
        }
        Ok((primary, front))
    }

    pub fn check(&self) -> Result<(), String> {
        if !(2..=32).contains(&self.packages.len()) {
            return Err("packages must contain 2 to 32 entries".into());
        }
        self.selected()?;
        if self.packages[&self.front_package]
            .dependencies
            .get("service")
            .is_some_and(|target| target != &self.primary_package)
        {
            return Err("Front service dependency must select primary_package".into());
        }
        let mut digests = BTreeMap::new();
        for (name, package) in &self.packages {
            if !crate::uri::is_realm_id(name) {
                return Err(format!("invalid package reference {name:?}"));
            }
            package
                .check()
                .map_err(|e| format!("packages[{name}]: {e}"))?;
            if package.local_path.is_some()
                && (name != &self.front_package
                    || package.dependencies.get("service") != Some(&self.primary_package))
            {
                return Err(
                    "local_path is only allowed on the selected Front with a Service binding"
                        .into(),
                );
            }
            if digests
                .insert(&package.r#type, &package.sha256)
                .is_some_and(|old| old != &package.sha256)
            {
                return Err(format!("conflicting digest for {}", package.r#type));
            }
            for (alias, target) in &package.dependencies {
                if alias.is_empty() || alias.trim() != alias {
                    return Err(format!(
                        "packages[{name}].dependencies has an empty or untrimmed alias"
                    ));
                }
                if !self.packages.contains_key(target) {
                    return Err(format!(
                        "packages[{name}].dependencies[{alias}] names unknown package reference {target:?}"
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn check_successor(&self, next: &Self) -> Result<(), String> {
        self.selected()?
            .0
            .check_primary_successor(next.selected()?.0)?;
        let digests: BTreeMap<_, _> = self
            .packages
            .values()
            .filter_map(|p| p.fixed_digest().map(|digest| (&p.r#type, digest)))
            .collect();
        for p in next.packages.values() {
            if let Some(digest) = p.fixed_digest()
                && digests.get(&p.r#type).is_some_and(|old| *old != digest)
            {
                return Err(format!("digest changed for immutable package {}", p.r#type));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn selection() -> RealmPackageSelection {
        serde_json::from_value(json!({
            "primary_package": "service",
            "front_package": "front",
            "packages": {
                "service": {
                    "href": "https://publisher.example/service.actr",
                    "sha256": "a".repeat(64),
                    "type": "publisher.example/service@1.0.0"
                },
                "front": {
                    "href": "front.actr",
                    "type": "publisher.example/front@1.0.0",
                    "dependencies": { "service": "service" },
                    "local_path": {
                        "signing_key_id": "actr-pub-0123456789abcdef",
                        "allow_rebuild": true
                    }
                }
            }
        }))
        .unwrap()
    }

    #[test]
    fn local_front_requires_an_explicit_service_binding_and_safe_hint() {
        let mut selection = selection();
        selection.check().unwrap();
        assert_eq!(selection.selected().unwrap().1.fixed_digest(), None);

        selection.packages.get_mut("front").unwrap().href = "/Users/test/My App/front.actr".into();
        selection.check().unwrap();
        for invalid in [
            "project/front.actr",
            "//server/share/front.actr",
            "file:///Users/test/front.actr",
            "/Users/test/../front.actr",
            "/Users/test/%2e%2e/front.actr",
            "/Users/test//front.actr",
            "/Users/test/.actr",
            "/Users/test/front.unsigned",
            "/dev/front.actr",
            "/System/Library/front.actr",
            "/private/etc/front.actr",
            "/private/var/root/front.actr",
            "/Users/test/\nfront.actr",
            "/Users/test/\u{202e}front.actr",
        ] {
            selection.packages.get_mut("front").unwrap().href = invalid.into();
            assert!(selection.check().is_err(), "{invalid:?}");
        }

        let front = selection.packages.get_mut("front").unwrap();
        front.href = "front.actr".into();
        front.dependencies.clear();
        assert!(selection.check().is_err());
    }

    #[test]
    fn rebuild_permission_does_not_apply_to_downloads_or_services() {
        let mut selection = selection();
        let local = selection.packages["front"].local_path.clone();
        selection.packages.get_mut("service").unwrap().local_path = local;
        assert!(selection.check().is_err());

        let mut selection = self::selection();
        let front = selection.packages.get_mut("front").unwrap();
        front.href = "https://publisher.example/front.actr".into();
        front.local_path = None;
        assert!(selection.check().is_err());
    }
}
