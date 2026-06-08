(define (assoc-set alist key value)
  (cond ((null? alist) (list (cons key value)))
        ((eq? (caar alist) key) (cons (cons key value) (cdr alist)))
        (else (cons (car alist) (assoc-set (cdr alist) key value)))))

(define (trie-get trie path)
  (cond ((null? path) trie)
        ((not (pair? trie)) '(nothing))
        (else (let ((entry (assoc (car path) trie)))
                (if entry (trie-get (cdr entry) (cdr path)) '(nothing))))))

(define (trie-set trie path value)
  (if (null? path)
      value
      (let* ((key (car path))
             (entry (and (pair? trie) (assoc key trie)))
             (child (if entry (cdr entry) '())))
        (assoc-set (if (pair? trie) trie '()) key (trie-set child (cdr path) value)))))

(define t0 '())
(define t1 (trie-set t0 '(alice docs note) #u(104 105)))
(define t2 (trie-set t1 '(alice meta note) '((mime "text/plain"))))
(define t3 (trie-set t2 '(bob docs note) '(unknown)))
(list t3 (trie-get t3 '(alice docs note)) (trie-get t3 '(missing)) (trie-get t3 '(bob docs note)))
