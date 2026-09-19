//! A suspended child owns its context binding only while it is being polled.
//! Dropping a cancelled poll restores the parent binding before returning.
use super::*;

pub(super) struct Scope<'a, 'db> {
    pub context: &'a mut ExecutionContext<'db>,
    previous: Option<ExecutionValue>,
    variable: ir::NonEmptyString,
}

impl<'a, 'db> Scope<'a, 'db> {
    pub fn new(context: &'a mut ExecutionContext<'db>, value: ExecutionValue) -> Self {
        let variable = ir::NonEmptyString::new("$context").expect("constant variable");
        let previous = context.variables.insert(variable.clone(), value);
        Self {
            context,
            previous,
            variable,
        }
    }
}

impl Drop for Scope<'_, '_> {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => {
                self.context.variables.insert(self.variable.clone(), value);
            }
            None => {
                self.context.variables.remove(&self.variable);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancelling_suspended_scope_restores_parent_binding() {
        let db = test_support::open_db("pull-cancel-scope").await;
        let mut context = ExecutionContext::new(&db, context::ParamBindings::default());
        let variable = ir::NonEmptyString::new("$context").unwrap();
        for previous in [None, Some(ExecutionValue::Count(7))] {
            if let Some(value) = previous.clone() {
                context.variables.insert(variable.clone(), value);
            }
            let mut future = Box::pin(async {
                let scope = Scope::new(&mut context, ExecutionValue::Count(99));
                assert_eq!(
                    scope.context.variable_value(&variable).unwrap(),
                    &ExecutionValue::Count(99)
                );
                futures::future::pending::<()>().await;
                drop(scope);
            });
            assert!(matches!(
                futures::poll!(&mut future),
                std::task::Poll::Pending
            ));
            drop(future);
            assert_eq!(context.variables.get(&variable), previous.as_ref());
        }
        db.close().await.unwrap();
    }
}
