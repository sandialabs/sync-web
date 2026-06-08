;; Imported from upstream s7test.scm line 15109.
;; Original form:
;; (test (let* ((l1 (list 1 2)) (l2 (list l1)))
;; 	(list-set! l1 0 l1)
;; 	(string=? (object->string l2) "(#1=(#1# 2))"))
;;       #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let* ((l1 (list 1 2)) (l2 (list l1)))
	(list-set! l1 0 l1)
	(string=? (object->string l2) "(#1=(#1# 2))")))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15109 actual expected ok?))
