;; Imported from upstream s7test.scm line 15162.
;; Original form:
;; (test (let* ((l1 (list 1 2))
;; 	     (v1 (vector 1 2))
;; 	     (l2 (list 1 l1 2))
;; 	     (v2 (vector l1 v1 l2)))
;; 	(vector-set! v1 0 v2)
;; 	(list-set! l1 1 l2)
;; 	(string=? (object->string v2) "#2=#(#1=(1 #3=(1 #1# 2)) #(#2# 2) #3#)"))
;;       #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let* ((l1 (list 1 2))
	     (v1 (vector 1 2))
	     (l2 (list 1 l1 2))
	     (v2 (vector l1 v1 l2)))
	(vector-set! v1 0 v2)
	(list-set! l1 1 l2)
	(string=? (object->string v2) "#2=#(#1=(1 #3=(1 #1# 2)) #(#2# 2) #3#)")))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15162 actual expected ok?))
