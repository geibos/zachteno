use uuid::Uuid;

use super::{CourseRole, Membership, require, resolve};
use crate::auth::session::{CurrentUser, Roles};
use crate::error::AppError;

fn user(superuser: bool, methodist: bool) -> CurrentUser {
    CurrentUser {
        id: Uuid::new_v4(),
        email: String::new(),
        display_name: String::new(),
        roles: Roles { superuser, methodist },
        email_verified: true,
        trusted_registration: false,
        has_password: true,
        session_id: Uuid::nil(),
    }
}

const NOBODY: Membership = Membership { is_owner: false, is_teacher: false, is_student: false };

#[test]
fn methodist_of_other_course_only_observes() {
    let role = resolve(&user(false, true), &NOBODY);
    assert_eq!(role, Some(CourseRole::Observer), "чужой курс — наблюдатель");
    assert!(matches!(require(role, CourseRole::can_edit), Err(AppError::Forbidden)), "править нельзя");
    assert!(require(role, CourseRole::can_view).is_ok(), "смотреть можно");
}

#[test]
fn combined_roles_take_the_strongest_on_course() {
    let teacher_and_student = Membership { is_owner: false, is_teacher: true, is_student: true };
    assert_eq!(resolve(&user(false, true), &teacher_and_student), Some(CourseRole::Teacher), "преподаватель");
}

#[test]
fn student_gets_not_found_for_results() {
    let student = Membership { is_owner: false, is_teacher: false, is_student: true };
    let role = resolve(&user(false, false), &student);
    assert!(matches!(require(role, CourseRole::can_view), Err(AppError::NotFound)), "как будто нет");
}

#[test]
fn stranger_has_no_role() {
    assert_eq!(resolve(&user(false, false), &NOBODY), None, "посторонний");
}
