// Copyright 2013 The gl-rs developers. For a full listing of the authors,
// refer to the AUTHORS file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

extern crate xml = "rust-xml";
extern crate collections;

use self::collections::TreeSet;
use std::cell::RefCell;
use std::collections::HashSet;
use std::fmt;
use std::from_str::FromStr;
use std::io::MemReader;
use std::slice::Items;

use self::xml::reader::events;

pub enum Ns { Gl, Glx, Wgl }

impl FromStr for Ns {
    fn from_str(s: &str) -> Option<Ns> {
        match s {
            "gl"  => Some(Gl),
            "glx" => Some(Glx),
            "wgl" => Some(Wgl),
            _     => None,
        }
    }
}

impl fmt::Show for Ns {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Gl  => write!(fmt, "gl"),
            Glx => write!(fmt, "glx"),
            Wgl => write!(fmt, "wgl"),
        }
    }
}

impl fmt::Char for Ns {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Gl  => write!(fmt, "Gl"),
            Glx => write!(fmt, "Glx"),
            Wgl => write!(fmt, "Wgl"),
        }
    }
}

fn trim_str<'a>(s: &'a str, trim: &str) -> &'a str {
    if s.starts_with(trim) { s.slice_from(trim.len()) } else { s }
}

fn trim_enum_prefix<'a>(ident: &'a str, ns: Ns) -> &'a str {
    match ns {
        Gl => trim_str(ident, "GL_"),
        Glx => trim_str(ident, "GLX_"),
        Wgl =>  trim_str(ident, "WGL_"),
    }
}

fn trim_cmd_prefix<'a>(ident: &'a str, ns: Ns) -> &'a str {
    match ns {
        Gl => trim_str(ident, "gl"),
        Glx => trim_str(ident, "glx"),
        Wgl =>  trim_str(ident, "wgl"),
    }
}

pub struct Registry {
    pub groups: Vec<Group>,
    pub enums: Vec<Enum>,
    pub cmds: Vec<Cmd>,
    pub features: Vec<Feature>,
    pub extensions: Vec<Extension>,
}

impl Registry {
    /// Generate a registry from the supplied XML string
    pub fn from_xml(data: &str, ns: Ns, filter: Option<Filter>) -> Registry {
        RegistryBuilder::parse(data, ns, filter)
    }

    /// Returns a set of all the types used in the supplied registry. This is useful
    /// for working out what conversions are needed for the specific registry.
    pub fn get_tys(&self) -> TreeSet<String> {
        let mut tys = TreeSet::new();
        for def in self.cmds.iter() {
            tys.insert(def.proto.ty.clone());
            for param in def.params.iter() {
                tys.insert(param.ty.clone());
            }
        }
        tys
    }

    pub fn enum_iter<'a>(&'a self) -> EnumIterator<'a> {
        EnumIterator {
            seen: HashSet::new(),
            iter: self.enums.iter(),
        }
    }

    pub fn cmd_iter<'a>(&'a self) -> CmdIterator<'a> {
        CmdIterator {
            seen: HashSet::new(),
            iter: self.cmds.iter(),
        }
    }
}

pub struct EnumIterator<'a> {
    seen: HashSet<String>,
    iter: Items<'a, Enum>,
}

impl<'a> Iterator<&'a Enum> for EnumIterator<'a> {
    fn next(&mut self) -> Option<&'a Enum> {
        self.iter.next().and_then(|def| {
            if !self.seen.contains(&def.ident) {
                self.seen.insert(def.ident.clone());
                Some(def)
            } else {
                self.next()
            }
        })
    }
}

pub struct CmdIterator<'a> {
    seen: HashSet<String>,
    iter: Items<'a, Cmd>,
}

impl<'a> Iterator<&'a Cmd> for CmdIterator<'a> {
    fn next(&mut self) -> Option<&'a Cmd> {
        self.iter.next().and_then(|def| {
            if !self.seen.contains(&def.proto.ident) {
                self.seen.insert(def.proto.ident.clone());
                Some(def)
            } else {
                self.next()
            }
        })
    }
}

pub struct Group {
    pub name: String,
    pub enums: Vec<String>,
}

pub struct EnumNs {
    pub namespace: String,
    pub group: Option<String>,
    pub ty: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub vendor: Option<String>,
    pub comment: Option<String>,
    pub defs: Vec<Enum>,
}

pub struct Enum {
    pub ident: String,
    pub value: String,
    pub alias: Option<String>,
    pub ty: Option<String>,
}

pub struct CmdNs {
    pub namespace: String,
    pub defs: Vec<Cmd>,
}

pub struct Binding {
    pub ident: String,
    pub ty: String,
    pub group: Option<String>,
}

pub struct Cmd {
    pub proto: Binding,
    pub params: Vec<Binding>,
    pub is_safe: bool,
    pub alias: Option<String>,
    pub vecequiv: Option<String>,
    pub glx: Option<GlxOpcode>,
}

#[deriving(Clone)]
pub struct Feature {
    pub api: String,
    pub name: String,
    pub number: String,
    pub requires: Vec<Require>,
    pub removes: Vec<Remove>,
}

#[deriving(Clone)]
pub struct Require {
    pub comment: Option<String>,
    /// A reference to the earlier types, by name
    pub enums: Vec<String>,
    /// A reference to the earlier types, by name
    pub commands: Vec<String>,
}

#[deriving(Clone)]
pub struct Remove {
    // always core, for now
    pub profile: String,
    pub comment: String,
    /// A reference to the earlier types, by name
    pub enums: Vec<String>,
    /// A reference to the earlier types, by name
    pub commands: Vec<String>,
}

#[deriving(Clone)]
pub struct Extension {
    pub name: String,
    /// which apis this extension is defined for (see Feature.api)
    pub supported: Vec<String>,
    pub requires: Vec<Require>,
}

pub struct GlxOpcode {
    pub ty: String,
    pub opcode: String,
    pub name: Option<String>,
    pub comment: Option<String>,
}

struct RegistryBuilder {
    pub ns: Ns,
    pub filter: Option<Filter>,
    pub port: RefCell<xml::reader::EventReader<MemReader>>,
}

pub struct Filter {
    pub extensions: Vec<String>,
    pub profile: String,
    pub version: String,
    pub api: String,
}

/// A big, ugly, imperative impl with methods that accumulates a Registry struct
impl<'a> RegistryBuilder {
    fn parse(data: &str, ns: Ns, filter: Option<Filter>) -> Registry {
        RegistryBuilder {
            ns: ns,
            filter: filter,
            port: RefCell::new(xml::reader::EventReader::new(MemReader::new(data.as_bytes().to_vec()))),
        }.consume_registry()
    }

    fn recv(&self) -> events::XmlEvent {
        for event in self.port.borrow_mut().events() {
            match event {
                events::StartDocument{ref version, ref encoding, ref standalone} => (),
                events::Comment(_) => (),
                events::Whitespace(_) => (),
                events::EndDocument => fail!("The end of the document has been reached"),
                events::Error(err) => fail!("XML error: {}", err),
                event => return event,
            }
        }

        unreachable!()
    }

    fn expect_characters(&self) -> String {
        match self.recv() {
            events::Characters(ref ch) => ch.clone(),
            msg => fail!("Expected characters, found: {}", msg.to_string()),
        }
    }

    fn expect_start_element(&self, n: &str) -> Vec<xml::common::Attribute> {
        match self.recv() {
            events::StartElement{ref name, ref attributes, ref namespace}
                if n == name.local_name.as_slice() => attributes.clone(),
            msg => fail!("Expected <{}>, found: {}", n, msg.to_string()),
        }
    }

    fn expect_end_element(&self, n: &str) {
        match self.recv() {
            events::EndElement{ref name} if n == name.local_name.as_slice() => (),
            msg => fail!("Expected </{}>, found: {}", n, msg.to_string()),
        }
    }

    fn skip_until(&self, event: events::XmlEvent) {
        loop {
            match self.recv() {
                events::EndDocument => fail!("Expected {}, but reached the end of the document.",
                                     event.to_string()),
                ref msg if *msg == event => break,
                _ => (),
            }
        }
    }

    fn consume_registry(&self) -> Registry {
        self.expect_start_element("registry");
        let mut registry = Registry {
            groups: Vec::new(),
            enums: Vec::new(),
            cmds: Vec::new(),
            features: Vec::new(),
            extensions: Vec::new(),
        };

        loop {
            match self.recv() {
                // ignores
                events::Characters(_) | events::Comment(_) => (),
                events::StartElement{ref name, ref attributes, ref namespace}
                    if name.local_name.as_slice() == "comment" =>
                        self.skip_until(events::EndElement { name: name.clone() }),
                events::StartElement{ref name, ref attributes, ref namespace}
                    if name.local_name.as_slice() == "types" =>
                        self.skip_until(events::EndElement { name: name.clone() }),

                // add groups
                events::StartElement{ref name, ref attributes, ref namespace} if name.local_name.as_slice() == "groups" => {
                    loop {
                        match self.recv() {
                            events::StartElement{ref name, ref attributes, ref namespace} if name.local_name.as_slice() == "group" => {
                                registry.groups.push(
                                    self.consume_group(get_attribute(attributes.as_slice(), "name").unwrap())
                                );
                            }
                            events::EndElement{ref name} if name.local_name.as_slice() == "groups" => break,
                            msg => fail!("Expected </groups>, found: {}", msg.to_string()),
                        }
                    }
                }

                // add enum namespace
                events::StartElement{ref name, ref attributes, ref namespace} if name.local_name.as_slice() == "enums" => {
                    registry.enums.extend(self.consume_enums().move_iter());
                }

                // add command namespace
                events::StartElement{ref name, ref attributes, ref namespace} if name.local_name.as_slice() == "commands" => {
                    registry.cmds.extend(self.consume_cmds().move_iter());
                }

                events::StartElement{ref name, ref attributes, ref namespace} if name.local_name.as_slice() == "feature" => {
                    debug!("Parsing feature: {}", attributes.as_slice());
                    registry.features.push(FromXML::convert(self, attributes.as_slice()));
                }

                events::StartElement{ref name, ref attributes, ref namespace} if name.local_name.as_slice() == "extensions" => {
                    loop {
                        match self.recv() {
                            events::StartElement{ref name, ref attributes, ref namespace} if name.local_name.as_slice() == "extension" => {
                                registry.extensions.push(FromXML::convert(self, attributes.as_slice()));
                            }
                            events::EndElement{ref name} if name.local_name.as_slice() == "extensions" => break,
                            msg => fail!("Unexpected message {}", msg.to_string()),
                        }
                    }
                }

                // finished building the registry
                events::EndElement{ref name} if name.local_name.as_slice() == "registry" => break,

                // error handling
                msg => fail!("Expected </registry>, found: {}", msg.to_string()),
            }
        }

        match self.filter {
            Some(ref filter) => {
                let Registry {
                    groups, enums, cmds, features: feats, extensions: exts
                } = registry;

                let mut desired_enums = HashSet::new();
                let mut desired_cmds = HashSet::new();

                // find the features we want
                let mut found_feat = false;
                for f in feats.iter() {
                    // XXX: verify that the string comparison with <= actually works as desired
                    if f.api == filter.api && f.number <= filter.version {
                        for req in f.requires.iter() {
                            desired_enums.extend(req.enums.iter().map(|x| x.clone()));
                            desired_cmds.extend(req.commands.iter().map(|x| x.clone()));
                        }
                    }
                    if f.number == filter.version {
                        found_feat = true;
                    }
                }

                // remove the things that should be removed
                for f in feats.iter() {
                    // XXX: verify that the string comparison with <= actually works as desired
                    if f.api == filter.api && f.number <= filter.version {
                        for rem in f.removes.iter() {
                            if rem.profile == filter.profile {
                                for enm in rem.enums.iter() {
                                    debug!("Removing {}", enm);
                                    desired_enums.remove(enm);
                                }
                                for cmd in rem.commands.iter() {
                                    debug!("Removing {}", cmd);
                                    desired_cmds.remove(cmd);
                                }
                            }
                        }
                    }
                }

                if !found_feat {
                    fail!("Did not find version {} in the registry", filter.version);
                }

                for ext in exts.iter() {
                    if filter.extensions.iter().any(|x| x == &ext.name) {
                        if !ext.supported.iter().any(|x| x == &filter.api) {
                            fail!("Requested {}, which doesn't support the {} API", ext.name, filter.api);
                        }
                        for req in ext.requires.iter() {
                            desired_enums.extend(req.enums.iter().map(|x| x.clone()));
                            desired_cmds.extend(req.commands.iter().map(|x| x.clone()));
                        }
                    }
                }

                Registry {
                    groups: groups,
                    enums: enums.move_iter().filter(|e| desired_enums.contains(&("GL_".to_string().append(e.ident.as_slice())))).collect::<Vec<Enum>>(),
                    cmds: cmds.move_iter().filter(|c| desired_cmds.contains(&("gl".to_string().append(c.proto.ident.as_slice())))).collect::<Vec<Cmd>>(),
                    // these aren't important after this step
                    features: Vec::new(),
                    extensions: Vec::new(),
                }
            },
            None => registry
        }
    }

    fn consume_two<'a, T: FromXML, U: FromXML>(&self, one: &'a str, two: &'a str, end: &'a str) -> (Vec<T>, Vec<U>) {
        debug!("consume_two: looking for {:s} and {:s} until {:s}", one, two, end);

        let mut ones = Vec::new();
        let mut twos = Vec::new();

        loop {
            match self.recv() {
                events::StartElement{ref name, ref attributes, ref namespace} => {
                    debug!("Found start element <{} {}>", name, attributes.as_slice());
                    debug!("one and two are {} and {}", one, two);

                    let n = name.clone();

                    if one == n.local_name.as_slice() {
                        ones.push(FromXML::convert(self, attributes.as_slice()));
                    } else if "type" == n.local_name.as_slice() {
                        // XXX: GL1.1 contains types, which we never care about anyway.
                        // Make sure consume_two doesn't get used for things which *do*
                        // care about type.
                        warn!("Ignoring type!");
                        continue;
                    } else if two == n.local_name.as_slice() {
                        twos.push(FromXML::convert(self, attributes.as_slice()));
                    } else {
                        fail!("Unexpected element");
                        fail!("Unexpected element: <{} {}>", n, attributes.as_slice());
                    }
                },
                events::EndElement{ref name} => {
                    debug!("Found end element </{}>", name);

                    if (&[one, two]).iter().any(|&x| x == name.local_name.as_slice()) {
                        continue;
                    } else if "type" == name.local_name.as_slice() {
                        // XXX: GL1.1 contains types, which we never care about anyway.
                        // Make sure consume_two doesn't get used for things which *do*
                        // care about type.
                        warn!("Ignoring type!");
                        continue;
                    } else if end == name.local_name.as_slice() {
                        return (ones, twos);
                    } else {
                        fail!("Unexpected end element {}", name.local_name);
                    }
                },
                msg => fail!("Unexpected message {}", msg.to_string()) }
        }
    }

    fn consume_group(&self, name: String) -> Group {
        let mut enms = Vec::new();
        loop {
            match self.recv() {
                events::StartElement{ref name, ref attributes, ref namespace} if name.local_name.as_slice() == "enum" => {
                    enms.push(get_attribute(attributes.as_slice(), "name").unwrap());
                    self.expect_end_element("enum");
                }
                events::EndElement{ref name} if name.local_name.as_slice() == "group" => break,
                msg => fail!("Expected </group>, found: {}", msg.to_string()),
            }
        }
        Group {
            name: name,
            enums: enms,
        }
    }

    fn consume_enums(&self) -> Vec<Enum> {
        let mut enums = Vec::new();
        loop {
            match self.recv() {
                // ignores
                events::Characters(_) | events::Comment(_) => (),
                events::StartElement{ref name, ref attributes, ref namespace} if name.local_name.as_slice() == "unused" =>
                    self.skip_until(events::EndElement{name: name.clone()}),

                // add enum definition
                events::StartElement{ref name, ref attributes, ref namespace} if name.local_name.as_slice() == "enum" => {
                    enums.push(
                        Enum {
                            ident:  trim_enum_prefix(get_attribute(attributes.as_slice(), "name").unwrap().as_slice(), self.ns).to_string(),
                            value:  get_attribute(attributes.as_slice(), "value").unwrap(),
                            alias:  get_attribute(attributes.as_slice(), "alias"),
                            ty:     get_attribute(attributes.as_slice(), "type"),
                        }
                    );
                    self.expect_end_element("enum");
                }

                // finished building the namespace
                events::EndElement{ref name} if name.local_name.as_slice() == "enums" => break,
                // error handling
                msg => fail!("Expected </enums>, found: {}", msg.to_string()),
            }
        }
        enums
    }

    fn consume_cmds(&self) -> Vec<Cmd> {
        let mut cmds = Vec::new();
        loop {
            match self.recv() {
                // add command definition
                events::StartElement{ref name, ref attributes, ref namespace} if name.local_name.as_slice() == "command" => {
                    cmds.push(self.consume_cmd());
                }
                // finished building the namespace
                events::EndElement{ref name} if name.local_name.as_slice() == "commands" => break,
                // error handling
                msg => fail!("Expected </commands>, found: {}", msg.to_string()),
            }
        }
        cmds
    }

    fn consume_cmd(&self) -> Cmd {
        // consume command prototype
        let proto_attr = self.expect_start_element("proto");
        let mut proto = self.consume_binding(get_attribute(proto_attr.as_slice(), "group"));
        proto.ident = trim_cmd_prefix(proto.ident.as_slice(), self.ns).to_string();
        self.expect_end_element("proto");

        let mut params = Vec::new();
        let mut alias = None;
        let mut vecequiv = None;
        let mut glx = None;
        loop {
            match self.recv() {
                events::StartElement{ref name, ref attributes, ref namespace} if name.local_name.as_slice() == "param" => {
                    params.push(
                        self.consume_binding(get_attribute(attributes.as_slice(), "group"))
                    );
                    self.expect_end_element("param");
                }
                events::StartElement{ref name, ref attributes, ref namespace} if name.local_name.as_slice() == "alias" => {
                    alias = get_attribute(attributes.as_slice(), "alias");
                    self.expect_end_element("alias");
                }
                events::StartElement{ref name, ref attributes, ref namespace} if name.local_name.as_slice() == "vecequiv" => {
                    vecequiv = get_attribute(attributes.as_slice(), "vecequiv");
                    self.expect_end_element("vecequiv");
                }
                events::StartElement{ref name, ref attributes, ref namespace} if name.local_name.as_slice() == "glx" => {
                    glx = Some(GlxOpcode {
                        ty:      get_attribute(attributes.as_slice(), "type").unwrap(),
                        opcode:  get_attribute(attributes.as_slice(), "opcode").unwrap(),
                        name:    get_attribute(attributes.as_slice(), "name"),
                        comment: get_attribute(attributes.as_slice(), "comment"),
                    });
                    self.expect_end_element("glx");
                }
                events::EndElement{ref name} if name.local_name.as_slice() == "command" => break,
                msg => fail!("Expected </command>, found: {}", msg.to_string()),
            }
        }
        let is_safe = params.len() <= 0 || params.iter().all(|p| !p.ty.as_slice().contains_char('*'));

        Cmd {
            proto: proto,
            params: params,
            is_safe: is_safe,
            alias: alias,
            vecequiv: vecequiv,
            glx: glx,
        }
    }

    fn consume_binding(&self, group: Option<String>) -> Binding {
        // consume type
        let mut ty = String::new();
        loop {
            match self.recv() {
                events::Characters(ch) => ty.push_str(ch.as_slice()),
                events::StartElement{ref name, ref attributes, ref namespace} if name.local_name.as_slice() == "ptype" => (),
                events::EndElement{ref name} if name.local_name.as_slice() == "ptype" => (),
                events::StartElement{ref name, ref attributes, ref namespace} if name.local_name.as_slice() == "name" => break,
                msg => fail!("Expected binding, found: {}", msg.to_string()),
            }
        }
        // consume identifier
        let ident = self.expect_characters();
        self.expect_end_element("name");
        Binding {
            ident: ident,
            ty: ty,
            group: group,
        }
    }
}

fn get_attribute(a: &[xml::common::Attribute], name: &str) -> Option<String> {
    a.iter().find(|a| a.name.local_name.as_slice() == name).map(|e| e.value.clone())
}

trait FromXML {
    fn convert(r: &RegistryBuilder, a: &[xml::common::Attribute]) -> Self;
}

impl FromXML for Require {
    fn convert(r: &RegistryBuilder, a: &[xml::common::Attribute]) -> Require {
        debug!("Doing a FromXML on Require");
        let comment = get_attribute(a, "comment");
        let (enums, commands) = r.consume_two("enum", "command", "require");
        Require {
            comment: comment,
            enums: enums,
            commands: commands
        }
    }
}

impl FromXML for Remove {
    fn convert(r: &RegistryBuilder, a: &[xml::common::Attribute]) -> Remove {
        debug!("Doing a FromXML on Remove");
        let profile = get_attribute(a, "profile").unwrap();
        let comment = get_attribute(a, "comment").unwrap();
        let (enums, commands) = r.consume_two("enum", "command", "remove");

        Remove {
            profile: profile,
            comment: comment,
            enums: enums,
            commands: commands
        }
    }
}

impl FromXML for Feature {
    fn convert(r: &RegistryBuilder, a: &[xml::common::Attribute]) -> Feature {
        debug!("Doing a FromXML on Feature");
        let api      = get_attribute(a, "api").unwrap();
        let name     = get_attribute(a, "name").unwrap();
        let number   = get_attribute(a, "number").unwrap();

        debug!("Found api = {:s}, name = {:s}, number = {:s}", api, name, number);

        let (require, remove) = r.consume_two("require", "remove", "feature");

        Feature {
            api: api,
            name: name,
            number: number,
            requires: require,
            removes: remove
        }
    }
}

impl FromXML for Extension {
    fn convert(r: &RegistryBuilder, a: &[xml::common::Attribute]) -> Extension {
        debug!("Doing a FromXML on Extension");
        let name = get_attribute(a, "name").unwrap();
        let supported = get_attribute(a, "supported").unwrap().as_slice().split('|').map(|x| x.to_string()).collect::<Vec<String>>();
        let mut require = Vec::new();
        loop {
            match r.recv() {
                events::StartElement{ref name, ref attributes, ref namespace} if name.local_name.as_slice() == "require" => {
                    require.push(FromXML::convert(r, attributes.as_slice()));
                }
                events::EndElement{ref name} if name.local_name.as_slice() == "extension" => break,
                msg => fail!("Unexpected message {}", msg.to_string())
            }
        }

        Extension {
            name: name,
            supported: supported,
            requires: require
        }
    }
}

impl FromXML for String {
    fn convert(_: &RegistryBuilder, a: &[xml::common::Attribute]) -> String {
        get_attribute(a, "name").unwrap()
    }
}
