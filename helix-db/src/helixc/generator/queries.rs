use std::fmt::{self, Display};

use crate::helixc::generator::{
    return_values::ReturnValue,
    statements::Statement,
    utils::{EmbedData, GeneratedType},
};

pub struct Query {
    pub embedding_model_to_use: Option<String>,
    pub mcp_handler: Option<String>,
    pub name: String,
    pub statements: Vec<Statement>,
    pub parameters: Vec<Parameter>, // iterate through and print each one
    pub sub_parameters: Vec<(String, Vec<Parameter>)>,
    pub return_values: Vec<(String, ReturnValue)>,
    pub is_mut: bool,
    pub hoisted_embedding_calls: Vec<EmbedData>,
}

impl Query {
    fn print_handler(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "#[handler]")
    }

    fn print_parameters(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (name, parameters) in &self.sub_parameters {
            writeln!(f, "#[derive(Serialize, Deserialize, Clone)]")?;
            writeln!(f, "pub struct {name} {{")?;
            for parameter in parameters {
                writeln!(f, "    pub {}: {},", parameter.name, parameter.field_type)?;
            }
            writeln!(f, "}}")?;
        }
        Ok(())
    }

    fn print_return_values(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // No struct generation needed when using json! macro
        Ok(())
    }

    fn print_input_struct(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "#[derive(Serialize, Deserialize, Clone)]")?;
        writeln!(f, "pub struct {}Input {{\n", self.name)?;
        write!(
            f,
            "{}",
            self.parameters
                .iter()
                .map(|p| format!("{p}"))
                .collect::<Vec<_>>()
                .join(",\n")
        )?;
        write!(f, "\n}}\n")?;
        Ok(())
    }

    fn print_hoisted_embedding_calls(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.hoisted_embedding_calls.is_empty() {
            writeln!(
                f,
                "Err(IoContFn::create_err(move |__internal_cont_tx, __internal_ret_chan| Box::pin(async move {{"
            )?;
            // ((({ })))

            for (i, embed) in self.hoisted_embedding_calls.iter().enumerate() {
                let name = EmbedData::name_from_index(i);
                writeln!(f, "let {name} = {embed};")?;
            }

            writeln!(
                f,
                "__internal_cont_tx.send_async((__internal_ret_chan, Box::new(move || {{"
            )?;
            // ((({ }))).await.expect("Cont Channel should be alive")

            for (i, _) in self.hoisted_embedding_calls.iter().enumerate() {
                let name = EmbedData::name_from_index(i);
                writeln!(f, "let {name}: Vec<f64> = {name}?;")?;
            }
        }
        Ok(())
    }

    fn print_txn_commit(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "txn.commit().map_err(|e| GraphError::New(format!(\"Failed to commit transaction: {{:?}}\", e)))?;"
        )
    }

    fn print_query(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // prints the function signature
        if !self.parameters.is_empty() {
            self.print_input_struct(f)?;
            self.print_parameters(f)?;
        }
        if !self.return_values.is_empty() {
            self.print_return_values(f)?;
        }

        self.print_handler(f)?;
        writeln!(
            f,
            "pub fn {} (input: HandlerInput) -> Result<Response, GraphError> {{",
            self.name
        )?;

        // print the db boilerplate
        writeln!(f, "let db = Arc::clone(&input.graph.storage);")?;
        if !self.parameters.is_empty() {
            match self.hoisted_embedding_calls.is_empty() {
                true => writeln!(
                    f,
                    "let data = input.request.in_fmt.deserialize::<{}Input>(&input.request.body)?;",
                    self.name
                )?,
                false => writeln!(
                    f,
                    "let data = input.request.in_fmt.deserialize::<{}Input>(&input.request.body)?.into_owned();",
                    self.name
                )?,
            }
        }
        
        // print embedding calls
        self.print_hoisted_embedding_calls(f)?;
        writeln!(f, "let arena = Bump::new();")?;
        
        match self.is_mut {
            true => writeln!(
                f,
                "let mut txn = db.graph_env.write_txn().map_err(|e| GraphError::New(format!(\"Failed to start write transaction: {{:?}}\", e)))?;"
            )?,
            false => writeln!(
                f,
                "let txn = db.graph_env.read_txn().map_err(|e| GraphError::New(format!(\"Failed to start read transaction: {{:?}}\", e)))?;"
            )?,
        }

        // prints each statement
        for statement in &self.statements {
            writeln!(f, "    {statement};")?;
        }

        self.print_txn_commit(f)?;

        // Generate return value using json! macro with field extraction
        if !self.return_values.is_empty() {
            write!(f, "let response = json!({{")?;
            for (i, (field_name, ret_val)) in self.return_values.iter().enumerate() {
                if i > 0 {
                    write!(f, ",")?;
                }
                writeln!(f)?;

                // If this return value has schema fields, extract them into json
                if !ret_val.fields.is_empty() {
                    write!(f, "    \"{}\": json!({{", field_name)?;
                    for (j, field) in ret_val.fields.iter().enumerate() {
                        if j > 0 {
                            write!(f, ",")?;
                        }
                        writeln!(f)?;
                        if field.name == "id" {
                            write!(f, "        \"{}\": uuid_str({}.id(), &arena)", field.name, field_name)?;
                        } else if field.name == "label" {
                            write!(f, "        \"{}\": {}.label()", field.name, field_name)?;
                        } else {
                            write!(f, "        \"{}\": {}.get_property(\"{}\").unwrap()",
                                field.name, field_name, field.name)?;
                        }
                    }
                    writeln!(f)?;
                    write!(f, "    }})")?;
                } else {
                    // For scalar or other types, serialize directly
                    // If there's a literal value, use it directly
                    if let Some(ref lit) = ret_val.literal_value {
                        write!(f, "    \"{}\": {}", field_name, lit)?;
                    } else {
                        write!(f, "    \"{}\": {}", field_name, field_name)?;
                    }
                }
            }
            writeln!(f)?;
            writeln!(f, "}});")?;
            writeln!(f, "Ok(input.request.out_fmt.create_response(&response))")?;
        } else {
            writeln!(f, "Ok(input.request.out_fmt.create_response(&()))")?;
        }

        if !self.hoisted_embedding_calls.is_empty() {
            writeln!(f, r#"}}))).await.expect("Cont Channel should be alive")"#)?;
            writeln!(f, "}})))")?;
        }
        writeln!(f, "}}")?;
        Ok(())
    }

    fn print_mcp(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.mcp_handler.is_none() {
            return Ok(());
        }

        let struct_name = format!("{}Input", self.name);
        let mcp_struct_name = format!("{}McpInput", self.name);
        let mcp_function_name = format!("{}Mcp", self.name);

        writeln!(f, "#[derive(Deserialize, Clone)]")?;
        writeln!(f, "pub struct {mcp_struct_name} {{")?;
        writeln!(f, "    connection_id: String,")?;
        if !self.parameters.is_empty() {
            writeln!(f, "    data: {struct_name},")?;
        } else {
            writeln!(f, "    data: serde_json::Value,")?;
        }
        writeln!(f, "}}")?;

        writeln!(f, "#[mcp_handler]")?;
        writeln!(
            f,
            "pub fn {mcp_function_name}(input: &mut MCPToolInput) -> Result<Response, GraphError> {{"
        )?;

        match self.hoisted_embedding_calls.is_empty() {
            true => writeln!(
                f,
                "let data = input.request.in_fmt.deserialize::<{mcp_struct_name}>(&input.request.body)?;"
            )?,
            false => writeln!(
                f,
                "let data = input.request.in_fmt.deserialize::<{mcp_struct_name}>(&input.request.body)?.into_owned();"
            )?,
        }

        writeln!(
            f,
            "let mut connections = input.mcp_connections.lock().map_err(|_| GraphError::Default)?;"
        )?;
        writeln!(
            f,
            "let mut connection = match connections.remove_connection(&data.connection_id) {{"
        )?;
        writeln!(f, "    Some(conn) => conn,")?;
        writeln!(f, "    None => return Err(GraphError::Default),")?;
        writeln!(f, "}};")?;
        writeln!(f, "drop(connections);")?;
        // print the db boilerplate
        writeln!(f, "let db = Arc::clone(&input.mcp_backend.db);")?;
        writeln!(f, "let arena = Bump::new();")?;
        match self.hoisted_embedding_calls.is_empty() {
            true => writeln!(f, "let data = &data.data;")?,
            false => writeln!(f, "let data = data.data;")?,
        }
        writeln!(f, "let connections = Arc::clone(&input.mcp_connections);")?;

        self.print_hoisted_embedding_calls(f)?;
        writeln!(f, "let arena = Bump::new();")?;

        match self.is_mut {
            true => writeln!(
                f,
                "let mut txn = db.graph_env.write_txn().map_err(|e| GraphError::New(format!(\"Failed to start write transaction: {{:?}}\", e)))?;"
            )?,
            false => writeln!(
                f,
                "let txn = db.graph_env.read_txn().map_err(|e| GraphError::New(format!(\"Failed to start read transaction: {{:?}}\", e)))?;"
            )?,
        }

        for statement in &self.statements {
            writeln!(f, "    {statement};")?;
        }
        self.print_txn_commit(f)?;

        // Generate return value using json! macro - same as regular handler
        if !self.return_values.is_empty() {
            write!(f, "let response = json!({{")?;
            for (i, (field_name, ret_val)) in self.return_values.iter().enumerate() {
                if i > 0 {
                    write!(f, ",")?;
                }
                writeln!(f)?;

                // If this return value has schema fields, extract them into json
                if !ret_val.fields.is_empty() {
                    write!(f, "    \"{}\": json!({{", field_name)?;
                    for (j, field) in ret_val.fields.iter().enumerate() {
                        if j > 0 {
                            write!(f, ",")?;
                        }
                        writeln!(f)?;
                        if field.name == "id" {
                            write!(f, "        \"{}\": uuid_str({}.id(), &arena)", field.name, field_name)?;
                        } else if field.name == "label" {
                            write!(f, "        \"{}\": {}.label()", field.name, field_name)?;
                        } else {
                            write!(f, "        \"{}\": {}.get_property(\"{}\").unwrap()",
                                field.name, field_name, field.name)?;
                        }
                    }
                    writeln!(f)?;
                    write!(f, "    }})")?;
                } else {
                    // For scalar or other types, serialize directly
                    // If there's a literal value, use it directly
                    if let Some(ref lit) = ret_val.literal_value {
                        write!(f, "    \"{}\": {}", field_name, lit)?;
                    } else {
                        write!(f, "    \"{}\": {}", field_name, field_name)?;
                    }
                }
            }
            writeln!(f)?;
            writeln!(f, "}});")?;
            writeln!(f, "let mut connections = connections.lock().unwrap();")?;
            writeln!(f, "connections.add_connection(connection);")?;
            writeln!(f, "drop(connections);")?;
            writeln!(f, "Ok(helix_db::protocol::format::Format::Json.create_response(&response))")?;
        } else {
            writeln!(f, "let mut connections = connections.lock().unwrap();")?;
            writeln!(f, "connections.add_connection(connection);")?;
            writeln!(f, "drop(connections);")?;
            writeln!(f, "Ok(helix_db::protocol::format::Format::Json.create_response(&()))")?;
        }
        if !self.hoisted_embedding_calls.is_empty() {
            writeln!(f, r#"}}))).await.expect("Cont Channel should be alive")"#)?;
            writeln!(f, "}})))")?;
        }
        writeln!(f, "}}")?;
        Ok(())
    }
}

impl Display for Query {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.print_query(f)?;
        self.print_mcp(f)
    }
}
impl Default for Query {
    fn default() -> Self {
        Self {
            embedding_model_to_use: None,
            mcp_handler: None,
            name: "".to_string(),
            statements: vec![],
            parameters: vec![],
            sub_parameters: vec![],
            return_values: vec![],
            is_mut: false,
            hoisted_embedding_calls: vec![],
        }
    }
}

impl Query {
    pub fn add_hoisted_embed(&mut self, embed_data: EmbedData) -> String {
        let name = EmbedData::name_from_index(self.hoisted_embedding_calls.len());
        self.hoisted_embedding_calls.push(embed_data);
        name
    }
}

pub struct Parameter {
    pub name: String,
    pub field_type: GeneratedType,
    pub is_optional: bool,
}
impl Display for Parameter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.is_optional {
            true => write!(f, "pub {}: Option<{}>", self.name, self.field_type),
            false => write!(f, "pub {}: {}", self.name, self.field_type),
        }
    }
}
