QUERY GetUsers() =>
    users <- N<User>::FIRST
    RETURN users