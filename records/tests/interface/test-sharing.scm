(lambda (make-interface-harness)

  (with-let (make-interface-harness :journals 3 :users '(alice bob carol))

    (define error-result?
      (lambda (result)
        (and (pair? result) (eq? (car result) 'error))))

    (define alice-principal
      '(journal-2 journal-1 *state* alice))
    (define carol-principal
      '(journal-2 journal-1 *state* carol))
    (define alice-rule
      `((principal ,alice-principal) (key-index (-10 -1))
        (path (private)) (get #t) (set! #t) (resolve #t)))

    (test-report)

    ;; Journal-3 owns Bob's public and private data. Ordinary child names remain
    ;; visible in ancestor listings, but each child value is authorized alone.
    (test-submit ((*journal* journal-3 'set!) '(*state* bob published) "public-v1") :expect #t)
    (test-submit ((*journal* journal-3 'set!) '(*state* bob private) "private-v1") :expect #t)
    (test-submit ((*journal* journal-3 'set!) '(*state* bob hidden) "hidden-v1") :expect #t)
    (test-submit ((*journal* journal-3 'set!) '(*state* bob *directory*) #t) :expect #t)
    (test-submit
      ((*journal* journal-3 'authorize!)
       '((user (*state* bob))
         (rule ((principal (*public*)) (path (published))
                (get #t) (set! #f) (resolve #t)))))
      :expect #t)
    (test-submit ((*journal* journal-3 'authorize!) `((user (*state* bob)) (rule ,alice-rule))) :expect #t)
    (test-submit ((*journal* journal-1 'set!) '(*state* alice seed) "origin") :expect #t)
    (test-submit ((*journal* journal-2 'set!) '(*state* bob seed) "middle") :expect #t)
    (for-each
      (lambda (journal)
        (test-submit ((*journal* journal 'step!)) :expect 1))
      (list journal-1 journal-2 journal-3))
    (test-report)

    ;; Commit the reverse route as the topology grows, then propagate the
    ;; terminal head back to the origin.
    (test-submit ((*journal* journal-1 'bridge!) journal-2) :schedule '(2 1) :expect #t)
    (test-report)
    (test-submit ((*journal* journal-2 'step!)) :expect 2)
    (test-submit ((*journal* journal-2 'bridge!) journal-3) :schedule '(1 2) :tick 1 :expect #t)
    (test-report)
    (test-submit ((*journal* journal-3 'step!)) :expect 2)
    (test-submit ((*journal* journal-2 'step!)) :expect 3)
    (test-submit ((*journal* journal-1 'bridge!) journal-2) :schedule '(2 1) :tick 1 :expect #t)
    (test-report)
    (test-submit ((*journal* journal-1 'step!)) :expect 2)
    (test-report)

    ;; Independent principals use the same terminal concurrently. Delays reverse
    ;; completion order without changing authorization or returned content.
    (test-submit
      ((alice journal-1 journal-2 journal-3 'get) '(*state* bob private))
      :schedule '(3 1 2 0 1 2) :expect "private-v1")
    (test-submit
      ((carol journal-1 journal-2 journal-3 'get) '(*state* bob published))
      :schedule '(1 3 0 2 2 0) :expect "public-v1")
    (test-submit
      ((bob journal-2 journal-3 'get) '(*state* bob published))
      :schedule '(2 1 1 0) :tick 1 :expect "public-v1")
    (test-submit ((bob journal-2 journal-3 'get) '(*state* bob private)) :schedule '(1 2 0 1) :expect error-result?)
    (test-report)

    ;; A permitted private write can overlap an unrelated public read. The
    ;; subsequent phase observes the completed terminal mutation.
    (test-submit
      ((alice journal-1 journal-2 journal-3 'set!)
       '(*state* bob private) "private-v2")
      :schedule '(3 1 2 1 0 2) :expect #t)
    (test-submit
      ((carol journal-1 journal-2 journal-3 'get) '(*state* bob published))
      :schedule '(1 2 1 0 2 1) :tick 1 :expect "public-v1")
    (test-report)

    (test-submit
      ((alice journal-1 journal-2 journal-3 'get) '(*state* bob private))
      :schedule '(2 1 0 2 1 0) :expect "private-v2")
    ;; Stage reflects the completed write immediately, while resolve remains on
    ;; the terminal state committed into the origin's selected history.
    (test-submit
      ((alice journal-1 journal-2 journal-3 'resolve)
       '(-1 *state* bob private) :pinned? #f :proof? #f)
      :schedule '(1 2 2 0 1 1) :expect "private-v1")
    (test-submit
      ((carol journal-1 journal-2 journal-3 'set!)
       '(*state* bob published) "forbidden")
      :schedule '(2 1 1 2 0 1) :expect error-result?)
    (test-submit ((alice journal-2 journal-3 'get) '(*state* bob private)) :schedule '(1 2 0 1) :expect error-result?)

    ;; Descendant authority admits ordinary ancestor listings for both the
    ;; private grantee and a public-only caller without exposing child values.
    (test-submit
      ((alice journal-1 journal-2 journal-3 'get) '(*state* bob))
      :schedule '(2 1 2 0 1 0)
      :expect (lambda (result)
                (and (eq? (car result) 'directory)
                     (assoc 'published (cadr result))
                     (assoc 'private (cadr result))
                     (assoc 'hidden (cadr result))
                     (not (assoc '*directory* (cadr result))))))
    (test-submit
      ((carol journal-1 journal-2 journal-3 'get) '(*state* bob))
      :schedule '(1 3 0 1 2 0)
      :expect (lambda (result)
                (and (eq? (car result) 'directory)
                     (assoc 'published (cadr result))
                     (assoc 'private (cadr result))
                     (assoc 'hidden (cadr result))
                     (not (assoc '*directory* (cadr result))))))
    (test-submit
      ((carol journal-1 journal-2 journal-3 'get) '(*state* bob hidden))
      :schedule '(2 1 0 2 1 0) :expect error-result?)
    (test-report)

    ;; Exercise Tree-native bytes through an authorized remote Stage path,
    ;; including deletion and recreation.
    (test-submit
      ((alice journal-1 journal-2 journal-3 'set!)
       '(*state* bob private) #u(0 1 2 255) :expression? #f)
      :schedule '(2 1 0 1 2 0) :expect #t)
    (test-report)
    (test-submit
      ((*journal* journal-3 'get) '(*state* bob private) :expression? #f)
      :expect #u(0 1 2 255))
    (test-submit
      ((alice journal-1 journal-2 journal-3 'get)
       '(*state* bob private) :expression? #f)
      :schedule '(1 2 1 0 2 0)
      :expect #u(0 1 2 255))
    (test-report)
    (test-submit
      ((alice journal-1 journal-2 journal-3 'set!)
       '(*state* bob private) '(nothing))
      :schedule '(2 1 0 2 1 0) :expect #t)
    (test-report)
    (test-submit
      ((alice journal-1 journal-2 journal-3 'get) '(*state* bob private))
      :schedule '(1 2 0 1 2 0) :expect '(nothing))
    (test-submit
      ((alice journal-1 journal-2 journal-3 'set!)
       '(*state* bob private) "private-v3")
      :schedule '(2 1 1 0 2 0) :expect #t)
    (test-submit
      ((carol journal-1 journal-2 journal-3 'get) '(*state* bob published))
      :schedule '(1 3 0 1 2 0) :tick 1 :expect "public-v1")
    (test-report)

    ;; Carol's independent grant remains live while Alice's policy is revoked,
    ;; restored read-only, and then upgraded to writable again.
    (define carol-rule
      `((principal ,carol-principal) (key-index (-10 -1))
        (path (hidden)) (get #t) (set! #f) (resolve #t)))
    (test-submit ((*journal* journal-3 'authorize!) `((user (*state* bob)) (rule ,carol-rule))) :expect #t)
    (test-submit ((*journal* journal-3 'deauthorize!) `((user (*state* bob)) (rule ,alice-rule))) :expect #t)
    (test-submit
      ((alice journal-1 journal-2 journal-3 'get) '(*state* bob private))
      :schedule '(2 1 0 2 1 0) :expect error-result?)
    (test-submit
      ((carol journal-1 journal-2 journal-3 'get) '(*state* bob hidden))
      :schedule '(1 2 1 0 2 0) :expect "hidden-v1")
    (test-report)

    (define alice-read-rule
      `((principal ,alice-principal) (key-index (-10 -1))
        (path (private)) (get #t) (set! #f) (resolve #t)))
    (test-submit ((*journal* journal-3 'authorize!) `((user (*state* bob)) (rule ,alice-read-rule))) :expect #t)
    (test-submit
      ((alice journal-1 journal-2 journal-3 'get) '(*state* bob private))
      :schedule '(2 1 0 1 2 0) :expect "private-v3")
    (test-submit
      ((alice journal-1 journal-2 journal-3 'set!)
       '(*state* bob private) "forbidden-read-only")
      :schedule '(1 2 1 0 2 0) :expect error-result?)
    (test-report)

    (test-submit ((*journal* journal-3 'deauthorize!) `((user (*state* bob)) (rule ,alice-read-rule))) :expect #t)
    (test-submit ((*journal* journal-3 'authorize!) `((user (*state* bob)) (rule ,alice-rule))) :expect #t)
    (test-submit
      ((alice journal-1 journal-2 journal-3 'set!)
       '(*state* bob private) "private-v4")
      :schedule '(2 1 0 2 1 0) :expect #t)
    (test-submit
      ((carol journal-1 journal-2 journal-3 'get) '(*state* bob hidden))
      :schedule '(1 2 1 0 2 0) :expect "hidden-v1")
    (test-report)
    (test-submit
      ((alice journal-1 journal-2 journal-3 'get) '(*state* bob private))
      :schedule '(2 1 0 1 2 0) :expect "private-v4")

    (test-report)))
