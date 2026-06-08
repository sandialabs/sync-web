(error (syntax-error (":allow-other-keys should be the last parameter: (~S ~S ...)" lambda* (:rest r :allow-other-keys (a (length r))))))
