;; Imported from upstream s7test.scm line 25176.
;; Original form:
;; (test (object->string
;;        (inlet 'a (let ((y 1)) (dilambda (lambda () y) (lambda (x) (set! y x))))) :readable)
;;       "(inlet :a (let ((y 1)) (dilambda (lambda () y) (lambda (x) (set! y x)))))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string
       (inlet 'a (let ((y 1)) (dilambda (lambda () y) (lambda (x) (set! y x))))) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (let ((y 1)) (dilambda (lambda () y) (lambda (x) (set! y x)))))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25176 actual expected ok?))
