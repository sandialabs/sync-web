;; Imported from upstream s7test.scm line 15178.
;; Original form:
;; (test (let* ((l1 (list 1 2))
;; 	     (l2 (list 3 4))
;; 	     (l3 (list 5 l1 6 l2 7)))
;; 	(set! (cdr (cdr l1)) l1)
;; 	(set! (cdr (cdr l2)) l2)
;; 	(string=? (object->string l3) "(5 #1=(1 2 . #1#) 6 #2=(3 4 . #2#) 7)"))
;;       #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let* ((l1 (list 1 2))
	     (l2 (list 3 4))
	     (l3 (list 5 l1 6 l2 7)))
	(set! (cdr (cdr l1)) l1)
	(set! (cdr (cdr l2)) l2)
	(string=? (object->string l3) "(5 #1=(1 2 . #1#) 6 #2=(3 4 . #2#) 7)")))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15178 actual expected ok?))
