(format #f "~{~A~^,~}" (let ((x (list 1 2))) (set-cdr! (cdr x) x) x))
