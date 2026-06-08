(define (assoc-set alist key value)
  (cond ((null? alist) (list (cons key value)))
        ((eq? (caar alist) key) (cons (cons key value) (cdr alist)))
        (else (cons (car alist) (assoc-set (cdr alist) key value)))))

(define (deep-set tree path value)
  (if (null? path)
      value
      (let* ((key (car path))
             (entry (assoc key tree))
             (child (if entry (cdr entry) '())))
        (assoc-set tree key (deep-set child (cdr path) value)))))

(deep-set '() '(alice docs note) #u(1 2 3))
