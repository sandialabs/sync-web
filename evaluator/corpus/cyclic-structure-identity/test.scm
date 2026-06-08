(let* ((p (cons 'a '()))
       (q (cons p p)))
  (set-cdr! p p)
  (list
    (pair? p)
    (eq? p (cdr p))
    (eq? (car q) (cdr q))
    (car (cdr (cdr p)))))
