//! Characters and personas.

use crate::commands::characters as c;
use crate::commands::personas as p;
use crate::models::DynamicJson;

crate::rpc_family! {
    "create_character" => c::create_character [state] (name: String, data: DynamicJson);
    "get_character" => c::get_character [state] (id: String);
    "list_characters" => c::list_characters [state] ();
    "update_character" => c::update_character [state] (id: String, name: Option<String>, data: Option<DynamicJson>, avatar_path: Option<String>);
    "delete_character" => c::delete_character [state] (id: String);
    "trash_character" => c::trash_character [state] (id: String);
    "restore_character" => c::restore_character [state] (id: String);

    "create_persona" => p::create_persona [state] (name: String, data: DynamicJson);
    "get_persona" => p::get_persona [state] (id: String);
    "list_personas" => p::list_personas [state] ();
    "update_persona" => p::update_persona [state] (id: String, name: Option<String>, data: Option<DynamicJson>, avatar_path: Option<String>);
    "delete_persona" => p::delete_persona [state] (id: String);
    "trash_persona" => p::trash_persona [state] (id: String);
    "restore_persona" => p::restore_persona [state] (id: String);
}
