;; Imported from upstream s7test.scm line 15171.
;; Original form:
;; (test (let ((l1 (list 1 2))
;; 	    (l2 (list 1 2)))
;; 	(set! (car l1) l2)
;; 	(set! (car l2) l1)
;; 	(object->string (list l1 l2)))
;;       "(#1=(#2=(#1# 2) 2) #2#)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((l1 (list 1 2))
	    (l2 (list 1 2)))
	(set! (car l1) l2)
	(set! (car l2) l1)
	(object->string (list l1 l2))))))
       (expected (upstream-safe (lambda () "(#1=(#2=(#1# 2) 2) #2#)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15171 actual expected ok?))
