// Copyright 2015 Brendan Zabarauskas and the gl-rs developers
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

extern crate xml;
extern crate khronos_api;

use std::collections::hash_map::Entry;
use std::collections::BTreeSet;
use std::collections::HashSet;
use std::collections::HashMap;
use std::io;
use self::xml::attribute::OwnedAttribute;
use self::xml::EventReader as XmlEventReader;
use self::xml::reader::XmlEvent;

use {Fallbacks, Api, Profile};
use registry::{Binding, Cmd, Enum, GlxOpcode, Registry};

fn api_from_str(src: &str) -> Result<Api, ()> {
    match src {
        "gl" => Ok(Api::Gl),
        "glx" => Ok(Api::Glx),
        "wgl" => Ok(Api::Wgl),
        "egl" => Ok(Api::Egl),
        "glcore" => Ok(Api::GlCore),
        "gles1" => Ok(Api::Gles1),
        "gles2" => Ok(Api::Gles2),
        _     => Err(()),
    }
}

fn profile_from_str(src: &str) -> Result<Profile, ()> {
    match src {
        "core" => Ok(Profile::Core),
        "compatibility" => Ok(Profile::Compatibility),
        _ => Err(()),
    }
}

fn trim_str<'a>(s: &'a str, trim: &str) -> &'a str {
    if s.starts_with(trim) { &s[trim.len()..] } else { s }
}

fn trim_enum_prefix<'a>(ident: &'a str, api: Api) -> &'a str {
    match api {
        Api::Gl | Api::GlCore | Api::Gles1 | Api::Gles2 => trim_str(ident, "GL_"),
        Api::Glx => trim_str(ident, "GLX_"),
        Api::Wgl =>  trim_str(ident, "WGL_"),
        Api::Egl =>  trim_str(ident, "EGL_"),
    }
}

fn trim_cmd_prefix<'a>(ident: &'a str, api: Api) -> &'a str {
    match api {
        Api::Gl | Api::GlCore | Api::Gles1 | Api::Gles2 => trim_str(ident, "gl"),
        Api::Glx => trim_str(ident, "glX"),
        Api::Wgl =>  trim_str(ident, "wgl"),
        Api::Egl =>  trim_str(ident, "egl"),
    }
}

fn merge_map(a: &mut HashMap<String, Vec<String>>, b: HashMap<String, Vec<String>>) {
    for (k, v) in b.into_iter() {
        match a.entry(k) {
            Entry::Occupied(mut ent) => { ent.get_mut().extend(v.into_iter()); },
            Entry::Vacant(ent) => { ent.insert(v); }
        }
    }
}

#[derive(Clone)]
struct Feature {
    pub api: Api,
    pub name: String,
    pub number: String,
    pub requires: Vec<Require>,
    pub removes: Vec<Remove>,
}

#[derive(Clone)]
struct Require {
    /// A reference to the earlier types, by name
    pub enums: Vec<String>,
    /// A reference to the earlier types, by name
    pub commands: Vec<String>,
}

#[derive(Clone)]
struct Remove {
    // always Core, for now
    pub profile: Profile,
    /// A reference to the earlier types, by name
    pub enums: Vec<String>,
    /// A reference to the earlier types, by name
    pub commands: Vec<String>,
}

#[derive(Clone)]
struct Extension {
    pub name: String,
    /// which apis this extension is defined for (see Feature.api)
    pub supported: Vec<Api>,
    pub requires: Vec<Require>,
}

pub struct RegistryParser<R: io::Read> {
    api: Api,
    reader: XmlEventReader<R>,
}

pub struct Filter {
    pub fallbacks: Fallbacks,
    pub extensions: BTreeSet<String>,
    pub profile: Profile,
    pub version: String,
}

/// A big, ugly, imperative impl with methods that accumulates a Registry struct
impl<R: io::Read> RegistryParser<R> {
    fn next(&mut self) -> XmlEvent {
        loop {
            let event = self.reader.next();
            match event.unwrap() {
                XmlEvent::StartDocument { .. } => (),
                XmlEvent::EndDocument => panic!("The end of the document has been reached"),
                XmlEvent::Comment(_) => (),
                XmlEvent::Whitespace(_) => (),
                event => return event,
            }
        }
    }

    fn consume_characters(&mut self) -> String {
        match self.next() {
            XmlEvent::Characters(ch) => ch,
            msg => panic!("Expected characters, found: {:?}", msg),
        }
    }

    fn consume_start_element(&mut self, n: &str) -> Vec<OwnedAttribute> {
        match self.next() {
            XmlEvent::StartElement { name, attributes, .. } => {
                if n == name.local_name { attributes } else {
                    panic!("Expected <{}>, found: <{}>", n, name.local_name)
                }
            }
            msg => panic!("Expected <{}>, found: {:?}", n, msg),
        }
    }

    fn consume_end_element(&mut self, n: &str) {
        match self.next() {
            XmlEvent::EndElement { ref name } if n == name.local_name => (),
            msg => panic!("Expected </{}>, found: {:?}", n, msg),
        }
    }

    fn skip_to_end(&mut self, name: &str) {
        loop {
            match self.next() {
                XmlEvent::EndDocument => panic!("Expected </{}>, but reached the end of the document.", name),
                XmlEvent::EndElement { name: ref n } if n.local_name == name => break,
                _ => (),
            }
        }
    }

    pub fn parse(src: R, api: Api, filter: Filter) -> Registry {
        let mut parser = RegistryParser {
            api: api,
            reader: XmlEventReader::new(src),
        };

        parser.consume_start_element("registry");

        let mut enums = Vec::new();
        let mut cmds = Vec::new();
        let mut features = Vec::new();
        let mut extensions = Vec::new();
        let mut aliases = HashMap::new();

        loop {
            match parser.next() {
                // ignores
                XmlEvent::Characters(_) | XmlEvent::Comment(_) => (),
                XmlEvent::StartElement { ref name, .. } if name.local_name == "comment" => parser.skip_to_end("comment"),
                XmlEvent::StartElement { ref name, .. } if name.local_name == "types" => parser.skip_to_end("types"),
                XmlEvent::StartElement { ref name, .. } if name.local_name == "groups" => parser.skip_to_end("groups"),

                // add enum namespace
                XmlEvent::StartElement{ref name, ..} if name.local_name == "enums" => {
                    enums.extend(parser.consume_enums().into_iter());
                }

                // add command namespace
                XmlEvent::StartElement{ref name, ..} if name.local_name == "commands" => {
                    let (new_cmds, new_aliases) = parser.consume_cmds();
                    cmds.extend(new_cmds.into_iter());
                    merge_map(&mut aliases, new_aliases);
                }

                XmlEvent::StartElement{ref name, ref attributes, ..} if name.local_name == "feature" => {
                    debug!("Parsing feature: {:?}", attributes);
                    features.push(Feature::convert(&mut parser, &attributes));
                }

                XmlEvent::StartElement{ref name, ..} if name.local_name == "extensions" => {
                    loop {
                        match parser.next() {
                            XmlEvent::StartElement{ref name, ref attributes, ..} if name.local_name == "extension" => {
                                extensions.push(Extension::convert(&mut parser, &attributes));
                            }
                            XmlEvent::EndElement{ref name} if name.local_name == "extensions" => break,
                            msg => panic!("Unexpected message {:?}", msg),
                        }
                    }
                }

                // finished building the registry
                XmlEvent::EndElement{ref name} if name.local_name == "registry" => break,

                // error handling
                msg => panic!("Expected </registry>, found: {:?}", msg),
            }
        }

        let mut desired_enums = HashSet::new();
        let mut desired_cmds = HashSet::new();

        // find the features we want
        let mut found_feature = false;
        for f in features.iter() {
            // XXX: verify that the string comparison with <= actually works as desired
            if f.api == api && f.number <= filter.version {
                for require in f.requires.iter() {
                    desired_enums.extend(require.enums.iter().map(|x| x.clone()));
                    desired_cmds.extend(require.commands.iter().map(|x| x.clone()));
                }

                for remove in f.removes.iter() {
                    if remove.profile == filter.profile {
                        for enm in remove.enums.iter() {
                            debug!("Removing {}", enm);
                            desired_enums.remove(enm);
                        }
                        for cmd in remove.commands.iter() {
                            debug!("Removing {}", cmd);
                            desired_cmds.remove(cmd);
                        }
                    }
                }
            }
            if f.number == filter.version {
                found_feature = true;
            }
        }

        if !found_feature {
            panic!("Did not find version {} in the registry", filter.version);
        }

        for extension in extensions.iter() {
            if filter.extensions.contains(&extension.name) {
                if !extension.supported.contains(&api) {
                    panic!("Requested {}, which doesn't support the {} API", extension.name, api);
                }
                for require in extension.requires.iter() {
                    desired_enums.extend(require.enums.iter().map(|x| x.clone()));
                    desired_cmds.extend(require.commands.iter().map(|x| x.clone()));
                }
            }
        }

        let is_desired_enum = |e: &Enum| {
            desired_enums.contains(&("GL_".to_string() + &e.ident)) ||
            desired_enums.contains(&("WGL_".to_string() + &e.ident)) ||
            desired_enums.contains(&("GLX_".to_string() + &e.ident)) ||
            desired_enums.contains(&("EGL_".to_string() + &e.ident))
        };

        let is_desired_cmd = |c: &Cmd| {
            desired_cmds.contains(&("gl".to_string() + &c.proto.ident)) ||
            desired_cmds.contains(&("wgl".to_string() + &c.proto.ident)) ||
            desired_cmds.contains(&("glX".to_string() + &c.proto.ident)) ||
            desired_cmds.contains(&("egl".to_string() + &c.proto.ident))
        };

        Registry {
            api: api,
            enums: enums.into_iter().filter(is_desired_enum).collect(),
            cmds: cmds.into_iter().filter(is_desired_cmd).collect(),
            aliases: if filter.fallbacks == Fallbacks::None { HashMap::new() } else { aliases },
        }
    }

    fn consume_two<'a, T: FromXml, U: FromXml>(&mut self, one: &'a str, two: &'a str, end: &'a str) -> (Vec<T>, Vec<U>) {
        debug!("consume_two: looking for {} and {} until {}", one, two, end);

        let mut ones = Vec::new();
        let mut twos = Vec::new();

        loop {
            match self.next() {
                XmlEvent::StartElement{ref name, ref attributes, ..} => {
                    debug!("Found start element <{:?} {:?}>", name, attributes);
                    debug!("one and two are {} and {}", one, two);

                    let n = name.clone();

                    if one == n.local_name {
                        ones.push(FromXml::convert(self, &attributes));
                    } else if "type" == n.local_name {
                        // XXX: GL1.1 contains types, which we never care about anyway.
                        // Make sure consume_two doesn't get used for things which *do*
                        // care about type.
                        warn!("Ignoring type!");
                        continue;
                    } else if two == n.local_name {
                        twos.push(FromXml::convert(self, &attributes));
                    } else {
                        panic!("Unexpected element: <{:?} {:?}>", n, &attributes);
                    }
                },
                XmlEvent::EndElement{ref name} => {
                    debug!("Found end element </{:?}>", name);

                    if one == name.local_name || two == name.local_name {
                        continue;
                    } else if "type" == name.local_name {
                        // XXX: GL1.1 contains types, which we never care about anyway.
                        // Make sure consume_two doesn't get used for things which *do*
                        // care about type.
                        warn!("Ignoring type!");
                        continue;
                    } else if end == name.local_name {
                        return (ones, twos);
                    } else {
                        panic!("Unexpected end element {:?}", name.local_name);
                    }
                },
                msg => panic!("Unexpected message {:?}", msg) }
        }
    }

    fn consume_enums(&mut self) -> Vec<Enum> {
        let mut enums = Vec::new();
        loop {
            match self.next() {
                // ignores
                XmlEvent::Characters(_) | XmlEvent::Comment(_) => (),
                XmlEvent::StartElement { ref name, .. } if name.local_name == "unused" => self.skip_to_end("unused"),

                // add enum definition
                XmlEvent::StartElement{ref name, ref attributes, ..} if name.local_name == "enum" => {
                    enums.push(
                        Enum {
                            ident:  trim_enum_prefix(&get_attribute(&attributes, "name").unwrap(), self.api).to_string(),
                            value:  get_attribute(&attributes, "value").unwrap(),
                            alias:  get_attribute(&attributes, "alias"),
                            ty:     get_attribute(&attributes, "type"),
                        }
                    );
                    self.consume_end_element("enum");
                }

                // finished building the namespace
                XmlEvent::EndElement{ref name} if name.local_name == "enums" => break,
                // error handling
                msg => panic!("Expected </enums>, found: {:?}", msg),
            }
        }
        enums
    }

    fn consume_cmds(&mut self) -> (Vec<Cmd>, HashMap<String, Vec<String>>) {
        let mut cmds = Vec::new();
        let mut aliases: HashMap<String, Vec<String>> = HashMap::new();
        loop {
            match self.next() {
                // add command definition
                XmlEvent::StartElement { ref name, .. } if name.local_name == "command" => {
                    let new = self.consume_cmd();
                    if let Some(ref v) = new.alias {
                        match aliases.entry(v.clone()) {
                            Entry::Occupied(mut ent) => { ent.get_mut().push(new.proto.ident.clone()); },
                            Entry::Vacant(ent) => { ent.insert(vec![new.proto.ident.clone()]); }
                        }
                    }
                    cmds.push(new);
                }
                // finished building the namespace
                XmlEvent::EndElement{ref name} if name.local_name == "commands" => break,
                // error handling
                msg => panic!("Expected </commands>, found: {:?}", msg),
            }
        }
        (cmds, aliases)
    }

    fn consume_cmd(&mut self) -> Cmd {
        // consume command prototype
        self.consume_start_element("proto");
        let mut proto = self.consume_binding("proto");
        proto.ident = trim_cmd_prefix(&proto.ident, self.api).to_string();

        let mut params = Vec::new();
        let mut alias = None;
        let mut vecequiv = None;
        let mut glx = None;
        loop {
            match self.next() {
                XmlEvent::StartElement{ref name, ..} if name.local_name == "param" => {
                    params.push(self.consume_binding("param"));
                }
                XmlEvent::StartElement{ref name, ref attributes, ..} if name.local_name == "alias" => {
                    alias = get_attribute(&attributes, "name");
                    alias = alias.map(|t| trim_cmd_prefix(&t, self.api).to_string());
                    self.consume_end_element("alias");
                }
                XmlEvent::StartElement{ref name, ref attributes, ..} if name.local_name == "vecequiv" => {
                    vecequiv = get_attribute(&attributes, "vecequiv");
                    self.consume_end_element("vecequiv");
                }
                XmlEvent::StartElement{ref name, ref attributes, ..} if name.local_name == "glx" => {
                    glx = Some(GlxOpcode {
                        ty: get_attribute(&attributes, "type").unwrap(),
                        opcode: get_attribute(&attributes, "opcode").unwrap(),
                        name: get_attribute(&attributes, "name"),
                    });
                    self.consume_end_element("glx");
                }
                XmlEvent::EndElement{ref name} if name.local_name == "command" => break,
                msg => panic!("Expected </command>, found: {:?}", msg),
            }
        }

        Cmd {
            proto: proto,
            params: params,
            alias: alias,
            vecequiv: vecequiv,
            glx: glx,
        }
    }

    fn consume_binding(&mut self, outside_tag: &str) -> Binding {
        // consume type
        let mut ty = String::new();
        loop {
            match self.next() {
                XmlEvent::Characters(ch) => ty.push_str(&ch),
                XmlEvent::StartElement{ref name, ..} if name.local_name == "ptype" => (),
                XmlEvent::EndElement{ref name} if name.local_name == "ptype" => (),
                XmlEvent::StartElement{ref name, ..} if name.local_name == "name" => break,
                msg => panic!("Expected binding, found: {:?}", msg),
            }
        }

        // consume identifier
        let ident = self.consume_characters();
        self.consume_end_element("name");

        // consume the type suffix
        loop {
            match self.next() {
                XmlEvent::Characters(ch) => ty.push_str(&ch),
                XmlEvent::EndElement{ref name} if name.local_name == outside_tag => break,
                msg => panic!("Expected binding, found: {:?}", msg),
            }
        }

        Binding {
            ident: ident,
            ty: ty,
        }
    }
}

fn get_attribute(a: &[OwnedAttribute], name: &str) -> Option<String> {
    a.iter().find(|a| a.name.local_name == name).map(|e| e.value.clone())
}

trait FromXml {
    fn convert<R: io::Read>(r: &mut RegistryParser<R>, a: &[OwnedAttribute]) -> Self;
}

impl FromXml for Require {
    fn convert<R: io::Read>(r: &mut RegistryParser<R>, _: &[OwnedAttribute]) -> Require {
        debug!("Doing a FromXml on Require");
        let (enums, commands) = r.consume_two("enum", "command", "require");
        Require {
            enums: enums,
            commands: commands
        }
    }
}

impl FromXml for Remove {
    fn convert<R: io::Read>(r: &mut RegistryParser<R>, a: &[OwnedAttribute]) -> Remove {
        debug!("Doing a FromXml on Remove");
        let profile = get_attribute(a, "profile").unwrap();
        let profile = profile_from_str(&profile).unwrap();
        let (enums, commands) = r.consume_two("enum", "command", "remove");

        Remove {
            profile: profile,
            enums: enums,
            commands: commands
        }
    }
}

impl FromXml for Feature {
    fn convert<R: io::Read>(r: &mut RegistryParser<R>, a: &[OwnedAttribute]) -> Feature {
        debug!("Doing a FromXml on Feature");
        let api = get_attribute(a, "api").unwrap();
        let api = api_from_str(&api).unwrap();
        let name = get_attribute(a, "name").unwrap();
        let number = get_attribute(a, "number").unwrap();

        debug!("Found api = {}, name = {}, number = {}", api, name, number);

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

impl FromXml for Extension {
    fn convert<R: io::Read>(r: &mut RegistryParser<R>, a: &[OwnedAttribute]) -> Extension {
        debug!("Doing a FromXml on Extension");
        let name = get_attribute(a, "name").unwrap();
        let supported = get_attribute(a, "supported").unwrap()
            .split('|')
            .map(api_from_str)
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        let mut require = Vec::new();
        loop {
            match r.next() {
                XmlEvent::StartElement{ref name, ref attributes, ..} if name.local_name == "require" => {
                    require.push(FromXml::convert(r, &attributes));
                }
                XmlEvent::EndElement{ref name} if name.local_name == "extension" => break,
                msg => panic!("Unexpected message {:?}", msg)
            }
        }

        Extension {
            name: name,
            supported: supported,
            requires: require
        }
    }
}

impl FromXml for String {
    fn convert<R: io::Read>(_: &mut RegistryParser<R>, a: &[OwnedAttribute]) -> String {
        get_attribute(a, "name").unwrap()
    }
}