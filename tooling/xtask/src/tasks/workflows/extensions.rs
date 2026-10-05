use gh_workflow::Job;

use crate::tasks::workflows::vars;

pub(crate) trait WithAppSecrets: Sized {
    fn with_app_secrets(self) -> Self;
}

impl WithAppSecrets for Job {
    fn with_app_secrets(self) -> Self {
        self.add_secret("app-id", vars::ZZZ_ZIPPY_APP_ID)
            .add_secret("app-secret", vars::ZZZ_ZIPPY_APP_PRIVATE_KEY)
    }
}
