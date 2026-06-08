;; Imported from upstream s7test.scm line 15185.
;; Original form:
;; (test (let* ((lst1 (list 1 2))
;; 	     (lst2 (list (list (list 1 (list (list (list 2 (list (list (list 3 (list (list (list 4 lst1 5))))))))))))))
;; 	(set! (cdr (cdr lst1)) lst1)
;; 	(string=? (object->string lst2) "(((1 (((2 (((3 (((4 #1=(1 2 . #1#) 5))))))))))))"))
;;       #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let* ((lst1 (list 1 2))
	     (lst2 (list (list (list 1 (list (list (list 2 (list (list (list 3 (list (list (list 4 lst1 5))))))))))))))
	(set! (cdr (cdr lst1)) lst1)
	(string=? (object->string lst2) "(((1 (((2 (((3 (((4 #1=(1 2 . #1#) 5))))))))))))")))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15185 actual expected ok?))
