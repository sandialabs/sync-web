;; Imported from upstream s7test.scm line 15079.
;; Original form:
;; (test (let ((l (list 1 2)))
;; 	(list-set! l 0 l)
;; 	(string=? (object->string l) "#1=(#1# 2)"))
;;       #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((l (list 1 2)))
	(list-set! l 0 l)
	(string=? (object->string l) "#1=(#1# 2)")))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15079 actual expected ok?))
