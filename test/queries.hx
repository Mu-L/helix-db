N::File1 {
    INDEX name: String,
    age: I32,
}


QUERY file1(name: String, id: ID) =>
    user <- AddN<File1>({name: name, age: 50})
    u <- N<File1>(id)
    RETURN user
