N::User {
    name: String,
    age: I32,
}


QUERY aggregate(name: String, id: ID) =>
    users <- N<User>::AGGREGATE_BY(name, age)
    RETURN users

QUERY group_by(name: String, id: ID) =>
    users <- N<User>::GROUP_BY(name, age)
    RETURN users

QUERY count(name: String, id: ID) =>
    users1 <- N<User>::COUNT::GROUP_BY(name, age)
    users2 <- N<User>::COUNT::AGGREGATE_BY(name, age)
    RETURN users1, users2