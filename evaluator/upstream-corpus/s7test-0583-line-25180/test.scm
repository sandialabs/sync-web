;; Imported from upstream s7test.scm line 25180.
;; Original form:
;; (test (object->string (inlet 'a (open-input-string "123456")) :readable) "(inlet :a (open-input-string \"123456\"))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (open-input-string "123456")) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (open-input-string \"123456\"))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25180 actual expected ok?))
